use std::collections::HashMap;
use crate::assembler::binary::{Binary, BinaryBuilder};
use crate::assembler::binary::BinaryBuilderMode::Text;
use crate::assembler::directive::do_directive;
use crate::assembler::emit::do_instruction;
use crate::assembler::lexer::{Token, TokenKind};
use crate::assembler::lexer::TokenKind::{Symbol, Directive};
use crate::assembler::lexer_seek::{is_adjacent_kind, LexerSeek, LexerSeekPeekable};
use crate::assembler::instructions::Instruction;
use crate::assembler::instructions::instructions_map;
use crate::assembler::util::AssemblerReason::{UnexpectedToken, MissingRegion};
use crate::assembler::util::{AssemblerError, AssemblerReason};

fn do_symbol<'a, T: LexerSeekPeekable<'a>>(
    name: &'a str, iter: &mut T, builder: &mut BinaryBuilder, map: &HashMap<&str, &Instruction>
) -> Result<(), AssemblerReason> {
    // We need this region!

    let region = builder.region().ok_or(MissingRegion)?;

    match iter.seek_without(is_adjacent_kind) {
        Some(token) if token.kind == TokenKind::Colon => {
            iter.next(); // consume

            let pc = region.raw.address + region.raw.data.len() as u32;
            builder.labels.insert(name.to_string(), pc);

            Ok(())
        },
        _ => do_instruction(name, iter, builder, map)
    }
}

pub fn assemble<'a>(
    items: Vec<Token<'a>>, instructions: &[Instruction]
) -> Result<Binary, AssemblerError<'a>> {
    let mut iter = items.into_iter().peekable();

    let map = instructions_map(instructions);

    let mut builder = BinaryBuilder::new();
    builder.seek_mode(Text);

    while let Some(token) = iter.next_any() {
        let fail = |reason: AssemblerReason| AssemblerError {
            start: Some(token.start), reason
        };

        match token.kind {
            Directive(directive) => do_directive(directive, &mut iter, &mut builder),
            Symbol(name) => do_symbol(name, &mut iter, &mut builder, &map),
            _ => return Err(fail(UnexpectedToken))
        }.map_err(|reason| fail(reason))?
    }

    builder.build().map_err(|reason| AssemblerError { start: None, reason })
}
