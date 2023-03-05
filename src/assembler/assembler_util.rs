use std::error::Error;
use std::fmt::{Display, Formatter};
use TokenKind::NewLine;
use crate::assembler::binary::AddressLabel;
use crate::assembler::binary::AddressLabel::{Constant, Label};
use crate::assembler::lexer::{StrippedKind, Token, TokenKind};
use crate::assembler::lexer::TokenKind::{IntegerLiteral, Register, StringLiteral, Symbol, LeftBrace, RightBrace};
use crate::assembler::cursor::{is_adjacent_kind, LexerCursor};
use crate::assembler::registers::RegisterSlot;
use crate::assembler::assembler_util::InstructionValue::{Literal, Slot};

#[derive(Debug)]
pub enum AssemblerReason {
    UnexpectedToken(StrippedKind),
    EndOfFile,
    ExpectedRegister(StrippedKind),
    ExpectedConstant(StrippedKind),
    ExpectedString(StrippedKind),
    ExpectedLabel(StrippedKind),
    ExpectedNewline(StrippedKind),
    ExpectedLeftBrace(StrippedKind),
    ExpectedRightBrace(StrippedKind),
    ConstantOutOfRange(u64, u64), // start, end
    OverwriteEdge(u32, u64), // pc, count
    UnknownLabel(String),
    UnknownDirective(String),
    UnknownInstruction(String),
    JumpOutOfRange(u32, u32), // to, from
    MissingRegion,
    MissingInstruction
}

impl Display for AssemblerReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AssemblerReason::UnexpectedToken(kind) => write!(f, "Expected instruction or directive, but found {}", kind),
            AssemblerReason::EndOfFile => write!(f, "Assembler reached the end of the file, but requires an additional token here"),
            AssemblerReason::ExpectedRegister(kind) => write!(f, "Expected a register, but found {}", kind),
            AssemblerReason::ExpectedConstant(kind) => write!(f, "Expected an integer, but found {}", kind),
            AssemblerReason::ExpectedString(kind) => write!(f, "Expected a string literal, but found {}", kind),
            AssemblerReason::ExpectedLabel(kind) => write!(f, "Expected a label, but found {}", kind),
            AssemblerReason::ExpectedNewline(kind) => write!(f, "Expected a newline, but found {}", kind),
            AssemblerReason::ExpectedLeftBrace(kind) => write!(f, "Expected a left brace, but found {}", kind),
            AssemblerReason::ExpectedRightBrace(kind) => write!(f, "Expected a right brace, but found {}", kind),
            AssemblerReason::ConstantOutOfRange(min, max) => write!(f, "Constant must be between 0x{:x} and 0x{:x}", min, max),
            AssemblerReason::OverwriteEdge(pc, count) => write!(f, "Instruction pushes cursor out of boundary (from 0x{:} with {} bytes)", pc, count),
            AssemblerReason::UnknownLabel(name) => write!(f, "Could not find a label named \"{}\", check for typos", name),
            AssemblerReason::UnknownDirective(name) => write!(f, "There's no current support for any {} directive", name),
            AssemblerReason::UnknownInstruction(name) => write!(f, "Unknown instruction named \"{}\", check for typos", name),
            AssemblerReason::JumpOutOfRange(to, from) => write!(
                f, "Trying to jump to 0x{:08x} from 0x{:08x}, but this jump is too distant for this instruction", to, from),
            AssemblerReason::MissingRegion => write!(
                f, "Assembler did not mount a binary region. Please file an issue at https://github.com/1whatleytay/titan/issues"),
            AssemblerReason::MissingInstruction => write!(
                f, "Assembler marked an instruction that does not exist. Please file an issue at https://github.com/1whatleytay/titan/issues"),
        }
    }
}

#[derive(Debug)]
pub struct AssemblerError {
    pub start: Option<usize>,
    pub reason: AssemblerReason
}

impl Display for AssemblerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.reason.fmt(f)
    }
}

impl Error for AssemblerError { }

pub fn get_token<'a, 'b>(iter: &mut LexerCursor<'a, 'b>) -> Result<&'b Token<'a>, AssemblerError> {
    iter.next_adjacent().ok_or(AssemblerError { start: None, reason: AssemblerReason::EndOfFile })
}

fn default_error(reason: AssemblerReason, token: &Token) -> AssemblerError {
    let start = if token.kind == NewLine {
        None
    } else {
        Some(token.start)
    };

    AssemblerError { start, reason }
}

pub fn get_register<'a>(iter: &mut LexerCursor) -> Result<RegisterSlot, AssemblerError> {
    let token = get_token(iter)?;

    match token.kind {
        Register(slot) => Ok(slot),
        _ => Err(default_error(AssemblerReason::ExpectedRegister(token.kind.strip()), token))
    }
}

pub enum InstructionValue {
    Slot(RegisterSlot),
    Literal(u64)
}

// first -> pointed to but NOT consumed yet, this method call will consume it
pub fn get_integer(first: &Token, iter: &mut LexerCursor, consume: bool) -> Option<u64> {
    let start = iter.get_position();

    match &first.kind {
        TokenKind::Plus | TokenKind::Minus => {
            if consume {
                iter.next(); // consume first
            }

            let multiplier = if first.kind == TokenKind::Plus { 1i64 } else { -1i64 };

            let adjacent = iter.next_adjacent();

            if let Some(IntegerLiteral(value)) = adjacent.map(|t| &t.kind) {
                Some(((*value as i64) * multiplier) as u64)
            } else {
                iter.set_position(start);

                None
            }
        }
        IntegerLiteral(value) => {
            if consume {
                iter.next(); // consume first
            }

            Some(*value)
        }
        _ => None
    }
}

pub fn get_integer_adjacent(iter: &mut LexerCursor) -> Option<u64> {
    if let Some(token) = iter.seek_without(is_adjacent_kind) {
        get_integer(token, iter, true)
    } else {
        return None
    }
}

pub fn get_value<'a>(iter: &mut LexerCursor) -> Result<InstructionValue, AssemblerError> {
    let token = get_token(iter)?;

    if let Some(value) = get_integer(token, iter, false) {
        Ok(Literal(value))
    } else {
        match token.kind {
            Register(slot) => Ok(Slot(slot)),
            _ => Err(default_error(AssemblerReason::ExpectedRegister(token.kind.strip()), token))
        }
    }
}

pub fn maybe_get_value<'a>(
    iter: &mut LexerCursor
) -> Option<InstructionValue> {
    let Some(value) = iter.seek_without(is_adjacent_kind) else { return None };

    if let Some(value) = get_integer(value, iter, true) {
        return Some(Literal(value))
    } else {
        match value.kind {
            Register(slot) => {
                iter.next();

                Some(Slot(slot))
            },
            _ => None
        }
    }
}

pub fn get_constant<'a>(iter: &mut LexerCursor) -> Result<u64, AssemblerError> {
    let token = get_token(iter)?;

    if let Some(value) = get_integer(token, iter, false) {
        Ok(value)
    } else {
        Err(default_error(AssemblerReason::ExpectedConstant(token.kind.strip()), token))
    }
}

pub fn get_string<'a>(iter: &mut LexerCursor) -> Result<String, AssemblerError> {
    let token = get_token(iter)?;

    match &token.kind {
        StringLiteral(value) => Ok(value.clone()),
        _ => Err(default_error(AssemblerReason::ExpectedString(token.kind.strip()), token))
    }
}

fn to_label(token: &Token, iter: &mut LexerCursor) -> Result<AddressLabel, AssemblerError> {
    if let Some(value) = get_integer(token, iter, false) {
        Ok(Constant(value))
    } else {
        match &token.kind {
            Symbol(value) => Ok(Label(value.get().to_string(), token.start)),
            _ => Err(default_error(AssemblerReason::ExpectedLabel(token.kind.strip()), token))
        }
    }
}

pub fn get_label(iter: &mut LexerCursor) -> Result<AddressLabel, AssemblerError> {
    to_label(get_token(iter)?, iter)
}

pub enum OffsetOrLabel {
    Offset(u64, RegisterSlot),
    Address(AddressLabel)
}

pub fn get_offset_or_label<'a>(iter: &mut LexerCursor) -> Result<OffsetOrLabel, AssemblerError> {
    let token = get_token(iter)?;
    let value = get_integer(token, iter, false);

    let is_offset = iter.seek_without(is_adjacent_kind)
        .map(|token| token.kind == LeftBrace)
        .unwrap_or(false);

    if is_offset {
        let value = value.unwrap_or(0);

        iter.next(); // left brace

        let register = get_register(iter)?;

        let Some(right) = iter.next_adjacent() else {
            return Err(AssemblerError {
                start: None,
                reason: AssemblerReason::EndOfFile
            })
        };

        if right.kind != RightBrace {
            return Err(default_error(AssemblerReason::ExpectedRightBrace(right.kind.strip()), right))
        }

        Ok(OffsetOrLabel::Offset(value, register))
    } else {
        Ok(OffsetOrLabel::Address(to_label(token, iter)?))
    }
}

pub fn default_start(start: usize) -> impl Fn(AssemblerError) -> AssemblerError {
    move |error| {
        if error.start.is_none() {
            AssemblerError {
                start: Some(start),
                reason: error.reason
            }
        } else {
            error
        }
    }
}
