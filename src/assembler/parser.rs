use std::iter::Peekable;

use ::opcodes::OpCode;
use assembler::token::Token;

#[derive(Debug, PartialEq)]
pub struct ParserError {
    message: String,
}

impl ParserError {
    fn expected_instruction(line: u32) -> ParserError {
        ParserError::from(format!("Instruction expected. Line {}", line))
    }

    fn invalid_opcode_addressing_mode_combination(line: u32) -> ParserError {
        ParserError::from(format!("Invalid addressing mode for opcode. Line {}", line))
    }

    fn unexpected_eol(line: u32) -> ParserError {
        ParserError::from(format!("Unexpected end of line. Line {}", line))
    }
}

impl From<String> for ParserError {
    fn from(error: String) -> ParserError {
        ParserError { message: error }
    }
}

impl<'a> From<&'a str> for ParserError {
    fn from(error: &str) -> ParserError {
        ParserError { message: error.into() }
    }
}

pub struct Parser {
    tokens: Vec<Vec<Token>>,
    line: u32,
}

/// Parser processes a list of 6502 Assembly tokens
impl Parser {
    pub fn new(tokens: Vec<Vec<Token>>) -> Parser {
        Parser {
            tokens: tokens,
            line: 0,
        }
    }

    /// Processes its tokens and either returns them to the caller
    /// or produces an error
    pub fn parse(&mut self) -> Result<Vec<Vec<Token>>, ParserError> {
        for line in &self.tokens {
            self.line += 1;
            let mut peeker = line.iter().peekable();
            // Check what starts the line
            let token = peeker.peek().unwrap().clone();

            match *token {
                Token::Label(_) => {
                    // if its a label, consume it and move on
                    peeker.next();
                    if let None = peeker.peek() {
                        return Err(ParserError::expected_instruction(self.line));
                    }
                    let next = *peeker.peek().unwrap();
                    if let &Token::OpCode(_) = next {
                        Self::handle_opcode(&mut peeker, &&next, self.line)?;
                    } else {
                        return Err(ParserError::expected_instruction(self.line));
                    }
                }
                _ => (),
            }
        }
        Ok(self.tokens.iter().map(|v| v.clone()).collect())
    }

    fn handle_opcode<'a, I>(mut peeker: &mut Peekable<I>,
                            token: &Token,
                            line: u32)
                            -> Result<(), ParserError>
        where I: Iterator<Item = &'a Token>
    {
        if let None = peeker.peek() {
            Err(ParserError::unexpected_eol(line))
        } else {
            peeker.next();
            let next = *peeker.peek().unwrap();
            let addressing_mode = next.to_addressing_mode();
            if let &Token::OpCode(ref mnemonic) = token {
                if let Some(opcode) = OpCode::from_mnemonic_and_addressing_mode(mnemonic.clone(),
                                                                                addressing_mode) {
                    Ok(())
                } else {
                    Err(ParserError::invalid_opcode_addressing_mode_combination(line))
                }
            } else {
                Err(ParserError::expected_instruction(line))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::assembler::token::{ImmediateBase, Token};

    #[test]
    fn errors_on_multiple_labels() {
        let mut parser = Parser::new(vec![vec![Token::Label("MAIN".into()),
                                               Token::Label("METHOD".into()),
                                               Token::OpCode("LDA".into()),
                                               Token::Immediate("10".into(),
                                                                ImmediateBase::Base16)]]);

        assert_eq!(Err(ParserError::expected_instruction(1)), parser.parse());
    }

    #[test]
    fn does_not_error_on_single_label() {
        let mut parser = Parser::new(vec![vec![Token::Label("MAIN".into()),
                                               Token::OpCode("LDA".into()),
                                               Token::Immediate("10".into(),
                                                                ImmediateBase::Base16)]]);

        assert_eq!(&[Token::Label("MAIN".into()),
                     Token::OpCode("LDA".into()),
                     Token::Immediate("10".into(), ImmediateBase::Base16)],
                   &parser.parse().unwrap()[0][..]);
    }

    #[test]
    fn can_detect_invalid_addressing_modes() {
        let mut parser = Parser::new(vec![vec![Token::Label("MAIN".into()),
                                               Token::OpCode("LDX".into()),
                                               Token::IndirectY("10".into())]]);

        assert_eq!(Err(ParserError::invalid_opcode_addressing_mode_combination(1)),
                   parser.parse());
    }

    #[test]
    fn does_not_error_on_valid_addressing_modes() {
        let mut parser = Parser::new(vec![vec![Token::Label("MAIN".into()),
                                               Token::OpCode("LDA".into()),
                                               Token::IndirectY("10".into())]]);

        let result = parser.parse().unwrap();
        assert_eq!(&[Token::Label("MAIN".into()),
                     Token::OpCode("LDA".into()),
                     Token::IndirectY("10".into())],
                   &result[0][..]);
    }
}