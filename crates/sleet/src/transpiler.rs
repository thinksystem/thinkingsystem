// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2024 Jonathan Lee
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License version 3
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see https://www.gnu.org/licenses/.

pub mod flows {
    pub mod definition {
        use serde_json::Value;
        use std::collections::HashMap;
        #[derive(Debug, Clone)]
        pub enum BlockType {
            Conditional {
                condition: String,
                true_block: String,
                false_block: String,
            },
            Compute {
                expression: String,
                output_key: String,
                next_block: String,
            },
            AwaitInput {
                interaction_id: String,
                agent_id: String,
                prompt: String,
                state_key: String,
                next_block: String,
            },
            ForEach {
                loop_id: String,
                array_path: String,
                iterator_var: String,
                loop_body_block_id: String,
                exit_block_id: String,
            },
            TryCatch {
                try_block_id: String,
                catch_block_id: String,
            },
            Continue {
                loop_id: String,
            },
            Break {
                loop_id: String,
            },
            Terminate,
        }
        #[derive(Debug, Clone)]
        pub struct BlockDefinition {
            pub id: String,
            pub block_type: BlockType,
        }
        #[derive(Debug, Clone)]
        pub struct FlowDefinition {
            pub id: String,
            pub start_block_id: String,
            pub blocks: Vec<BlockDefinition>,
            pub participants: Vec<String>,
            pub permissions: HashMap<String, Vec<String>>,
            pub initial_state: Option<Value>,
            pub state_schema: Option<Value>,
        }
    }
}
pub mod orchestration {
    pub mod ast {
        use serde_json::Value;
        use std::collections::HashMap;
        #[derive(Debug, Clone, PartialEq)]
        pub enum Literal {
            Null,
            Bool(bool),
            Number(f64),
            String(String),
            Array(Vec<AstNode>),
            Object(HashMap<String, AstNode>),
        }
        impl TryFrom<Value> for Literal {
            type Error = String;
            fn try_from(value: Value) -> Result<Self, Self::Error> {
                match value {
                    Value::Null => Ok(Literal::Null),
                    Value::Bool(b) => Ok(Literal::Bool(b)),
                    Value::Number(n) => n
                        .as_f64()
                        .map(Literal::Number)
                        .ok_or_else(|| format!("Invalid f64 number: {n}")),
                    Value::String(s) => Ok(Literal::String(s)),
                    Value::Array(arr) => {
                        let nodes = arr
                            .into_iter()
                            .map(|v| {
                                Literal::try_from(v).map(|lit| AstNode::from(Op::Literal(lit)))
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok(Literal::Array(nodes))
                    }
                    Value::Object(obj) => {
                        let nodes = obj
                            .into_iter()
                            .map(|(k, v)| {
                                Literal::try_from(v).map(|lit| (k, AstNode::from(Op::Literal(lit))))
                            })
                            .collect::<Result<HashMap<_, _>, Self::Error>>()?;
                        Ok(Literal::Object(nodes))
                    }
                }
            }
        }
        #[derive(Debug, Clone, PartialEq)]
        pub enum PathSegment {
            State,
            Input,
            Key(String),
            Index(u64),
            DynamicOffset(Box<AstNode>),
        }
        #[derive(Debug, Clone, PartialEq)]
        pub struct AstNode {
            pub op: Op,
            pub metadata: HashMap<String, Value>,
        }
        impl From<Op> for AstNode {
            fn from(op: Op) -> Self {
                AstNode {
                    op,
                    metadata: HashMap::new(),
                }
            }
        }
        #[derive(Debug, Clone, PartialEq)]
        pub enum Op {
            Literal(Literal),
            Fetch(Vec<PathSegment>),
            Assign {
                path: Vec<PathSegment>,
                value: Box<AstNode>,
            },
            Sequence(Vec<AstNode>),
            If {
                condition: Box<AstNode>,
                then_branch: Box<AstNode>,
                else_branch: Option<Box<AstNode>>,
            },
            SetNextBlock(String),
            Await {
                interaction_id: String,
                agent_id: String,
                prompt: Option<Box<AstNode>>,
                timeout_ms: Option<u64>,
            },
            Terminate,
            PushErrorHandler {
                catch_block_id: String,
            },
            PopErrorHandler,
            Evaluate {
                bytecode: Vec<u8>,
                output_path: Vec<PathSegment>,
            },
            Length(Box<AstNode>),
            Add(Box<AstNode>, Box<AstNode>),
            LessThan(Box<AstNode>, Box<AstNode>),
        }
        #[derive(Debug, Clone)]
        pub struct Contract {
            pub version: String,
            pub initial_state: AstNode,
            pub start_block_id: String,
            pub blocks: HashMap<String, AstNode>,
            pub participants: Vec<String>,
            pub permissions: HashMap<String, Vec<String>>,
        }
    }
}
use crate::flows::definition::{BlockDefinition, BlockType, FlowDefinition};

use orchestration::ast::{AstNode, Contract, Literal, Op, PathSegment};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;
mod path_parser {
    use super::orchestration::ast::{AstNode, Literal, Op, PathSegment};
    use thiserror::Error;
    #[derive(Error, Debug, PartialEq, Clone)]
    #[error("parsing failed")]
    pub struct ParseError;
    fn parse_index_expression(s: &str) -> Result<AstNode, ParseError> {
        if let Ok(num) = s.parse::<f64>() {
            return Ok(AstNode::from(Op::Literal(Literal::Number(num))));
        }
        Ok(AstNode::from(Op::Fetch(vec![
            PathSegment::State,
            PathSegment::Key(s.to_string()),
        ])))
    }
    #[derive(Error, Debug, PartialEq)]
    pub enum PathParseError {
        #[error("Unexpected character '{0}' at position {1}")]
        UnexpectedCharacter(char, usize),
        #[error("Unterminated quoted key starting at position {0}")]
        UnterminatedQuotedKey(usize),
        #[error("Unterminated bracket expression starting at position {0}")]
        UnterminatedBracket(usize),
        #[error(
            "Content inside brackets is not a valid expression: '{content}'. Parse error: {source}"
        )]
        InvalidBracketContent {
            content: String,
            #[source]
            source: ParseError,
        },
    }
    pub fn transpile(path_str: &str) -> Result<Vec<PathSegment>, PathParseError> {
        let mut segments = Vec::new();
        let mut chars = path_str.chars().enumerate().peekable();
        let mut current_segment = String::new();
        while let Some((i, char)) = chars.next() {
            match char {
                '.' => {
                    if !current_segment.is_empty() {
                        segments.push(PathSegment::Key(std::mem::take(&mut current_segment)));
                    }
                }
                '[' => {
                    if !current_segment.is_empty() {
                        segments.push(PathSegment::Key(std::mem::take(&mut current_segment)));
                    }
                    let (segment, end_pos) = parse_bracket_content(path_str, i + 1)?;
                    segments.push(segment);
                    while let Some((next_i, _)) = chars.peek() {
                        if *next_i < end_pos {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                '"' | '\'' => {
                    if !current_segment.is_empty() {
                        return Err(PathParseError::UnexpectedCharacter(char, i));
                    }
                    let (key, end_pos) = parse_quoted_key(path_str, i, char)?;
                    segments.push(PathSegment::Key(key));
                    while let Some((next_i, _)) = chars.peek() {
                        if *next_i < end_pos {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                _ if char.is_alphanumeric() || char == '_' => {
                    current_segment.push(char);
                }
                _ => return Err(PathParseError::UnexpectedCharacter(char, i)),
            }
        }
        if !current_segment.is_empty() {
            segments.push(PathSegment::Key(current_segment));
        }
        Ok(segments)
    }
    fn parse_bracket_content(
        path_str: &str,
        start_pos: usize,
    ) -> Result<(PathSegment, usize), PathParseError> {
        let mut balance = 1;
        let mut end_pos = start_pos;
        let mut content = String::new();
        for (i, char) in path_str.chars().enumerate().skip(start_pos) {
            end_pos = i + 1;
            match char {
                '[' => balance += 1,
                ']' => {
                    balance -= 1;
                    if balance == 0 {
                        break;
                    }
                }
                _ => {}
            }
            content.push(char);
        }
        if balance != 0 {
            return Err(PathParseError::UnterminatedBracket(start_pos - 1));
        }
        if let Ok(index) = content.parse::<u64>() {
            return Ok((PathSegment::Index(index), end_pos));
        }
        let trimmed = content.trim();
        if (trimmed.starts_with('\'') && trimmed.ends_with('\''))
            || (trimmed.starts_with('"') && trimmed.ends_with('"'))
        {
            return Ok((
                PathSegment::Key(trimmed[1..trimmed.len() - 1].to_string()),
                end_pos,
            ));
        }
        match parse_index_expression(&content) {
            Ok(ast_node) => Ok((PathSegment::DynamicOffset(Box::new(ast_node)), end_pos)),
            Err(source) => Err(PathParseError::InvalidBracketContent { content, source }),
        }
    }
    fn parse_quoted_key(
        path_str: &str,
        start_pos: usize,
        quote_char: char,
    ) -> Result<(String, usize), PathParseError> {
        let mut key = String::new();
        let mut end_pos = start_pos + 1;
        for (i, char) in path_str.chars().enumerate().skip(end_pos) {
            end_pos = i + 1;
            if char == quote_char {
                return Ok((key, end_pos));
            }
            key.push(char);
        }
        Err(PathParseError::UnterminatedQuotedKey(start_pos))
    }
}
mod logging {
    #[allow(dead_code)]
    pub fn log_transpiler_event(_event: &str, _payload: serde_json::Value) {}
}
mod expression_compiler {
    use super::{FlowDefinition, TranspilerError};
    use crate::runtime::BytecodeAssembler;
    use serde_json::Value;
    use std::iter::Peekable;
    use std::str::Chars;
    #[derive(Debug, Clone, PartialEq)]
    enum Token {
        Identifier(String),
        Number(f64),
        String(String),
        True,
        False,
        Null,
        Plus,
        Minus,
        Star,
        Slash,
        Percent,
        EqEq,
        BangEq,
        Gt,
        GtEq,
        Lt,
        LtEq,
        And,
        Or,
        Bang,
        LParen,
        RParen,
        LBracket,
        RBracket,
        LBrace,
        RBrace,
        Comma,
        Colon,
        Question,
        Dot,
        Eof,
    }
    #[derive(Debug)]
    enum Expr {
        Literal(Value),
        Variable(Vec<String>),
        Binary {
            left: Box<Expr>,
            op: Token,
            right: Box<Expr>,
        },
        Unary {
            op: Token,
            expr: Box<Expr>,
        },
        Index {
            object: Box<Expr>,
            index: Box<Expr>,
        },
        Call {
            callee: Box<Expr>,
            args: Vec<Expr>,
        },
        Conditional {
            condition: Box<Expr>,
            then_expr: Box<Expr>,
            else_expr: Box<Expr>,
        },
        Grouping(Box<Expr>),
    }

    pub fn compile(
        expression: &str,
        flow_def: &FlowDefinition,
        block_id: &str,
    ) -> Result<Vec<u8>, TranspilerError> {
        let tokens = Tokenizer::new(expression).scan_tokens().map_err(|e| {
            TranspilerError::ExpressionParseError {
                block_id: block_id.to_string(),
                expr: expression.to_string(),
                error: e,
            }
        })?;
        let ast =
            Parser::new(tokens)
                .parse()
                .map_err(|e| TranspilerError::ExpressionParseError {
                    block_id: block_id.to_string(),
                    expr: expression.to_string(),
                    error: e,
                })?;
        if let Some(schema) = &flow_def.state_schema {
            validate_ast(&ast, schema, block_id, expression)?;
        }
        let bytecode =
            compile_ast_to_bytecode(&ast).map_err(|e| TranspilerError::ExpressionParseError {
                block_id: block_id.to_string(),
                expr: expression.to_string(),
                error: e,
            })?;
        Ok(bytecode)
    }
    struct Tokenizer<'a> {
        iter: Peekable<Chars<'a>>,
    }
    impl<'a> Tokenizer<'a> {
        fn new(input: &'a str) -> Self {
            Self {
                iter: input.chars().peekable(),
            }
        }
        fn scan_tokens(&mut self) -> Result<Vec<Token>, String> {
            let mut tokens = Vec::new();
            while let Some(&ch) = self.iter.peek() {
                match ch {
                    ' ' | '\r' | '\t' | '\n' => {
                        self.iter.next();
                    }
                    '+' => {
                        self.iter.next();
                        tokens.push(Token::Plus);
                    }
                    '-' => {
                        self.iter.next();
                        tokens.push(Token::Minus);
                    }
                    '*' => {
                        self.iter.next();
                        tokens.push(Token::Star);
                    }
                    '/' => {
                        self.iter.next();
                        tokens.push(Token::Slash);
                    }
                    '%' => {
                        self.iter.next();
                        tokens.push(Token::Percent);
                    }
                    '(' => {
                        self.iter.next();
                        tokens.push(Token::LParen);
                    }
                    ')' => {
                        self.iter.next();
                        tokens.push(Token::RParen);
                    }
                    '[' => {
                        self.iter.next();
                        tokens.push(Token::LBracket);
                    }
                    ']' => {
                        self.iter.next();
                        tokens.push(Token::RBracket);
                    }
                    '{' => {
                        self.iter.next();
                        tokens.push(Token::LBrace);
                    }
                    '}' => {
                        self.iter.next();
                        tokens.push(Token::RBrace);
                    }
                    ',' => {
                        self.iter.next();
                        tokens.push(Token::Comma);
                    }
                    ':' => {
                        self.iter.next();
                        tokens.push(Token::Colon);
                    }
                    '?' => {
                        self.iter.next();
                        tokens.push(Token::Question);
                    }
                    '.' => {
                        self.iter.next();
                        tokens.push(Token::Dot);
                    }
                    '=' => {
                        self.iter.next();
                        if let Some(&'=') = self.iter.peek() {
                            self.iter.next();
                            tokens.push(Token::EqEq);
                        } else {
                            return Err("Expected '=' after '='".to_string());
                        }
                    }
                    '!' => {
                        self.iter.next();
                        if let Some(&'=') = self.iter.peek() {
                            self.iter.next();
                            tokens.push(Token::BangEq);
                        } else {
                            tokens.push(Token::Bang);
                        }
                    }
                    '>' => {
                        self.iter.next();
                        if let Some(&'=') = self.iter.peek() {
                            self.iter.next();
                            tokens.push(Token::GtEq);
                        } else {
                            tokens.push(Token::Gt);
                        }
                    }
                    '<' => {
                        self.iter.next();
                        if let Some(&'=') = self.iter.peek() {
                            self.iter.next();
                            tokens.push(Token::LtEq);
                        } else {
                            tokens.push(Token::Lt);
                        }
                    }
                    '&' => {
                        self.iter.next();
                        if let Some(&'&') = self.iter.peek() {
                            self.iter.next();
                            tokens.push(Token::And);
                        } else {
                            return Err("Unexpected character: &".to_string());
                        }
                    }
                    '|' => {
                        self.iter.next();
                        if let Some(&'|') = self.iter.peek() {
                            self.iter.next();
                            tokens.push(Token::Or);
                        } else {
                            return Err("Unexpected character: |".to_string());
                        }
                    }
                    '"' => tokens.push(self.scan_string()?),
                    c if c.is_ascii_digit() => tokens.push(self.scan_number()?),
                    c if c.is_alphabetic() || c == '_' => tokens.push(self.scan_identifier()?),
                    _ => return Err(format!("Unexpected character: {ch}")),
                }
            }
            tokens.push(Token::Eof);
            Ok(tokens)
        }
        fn scan_string(&mut self) -> Result<Token, String> {
            self.iter.next();
            let mut value = String::new();
            for ch in self.iter.by_ref() {
                if ch == '"' {
                    return Ok(Token::String(value));
                }
                value.push(ch);
            }
            Err("Unterminated string".to_string())
        }
        fn scan_number(&mut self) -> Result<Token, String> {
            let mut value = String::new();
            while let Some(&ch) = self.iter.peek() {
                if ch.is_ascii_digit() || ch == '.' {
                    value.push(self.iter.next().unwrap());
                } else {
                    break;
                }
            }
            value
                .parse::<f64>()
                .map(Token::Number)
                .map_err(|_| "Invalid number".to_string())
        }
        fn scan_identifier(&mut self) -> Result<Token, String> {
            let mut value = String::new();
            while let Some(&ch) = self.iter.peek() {
                if ch.is_alphanumeric() || ch == '_' {
                    value.push(self.iter.next().unwrap());
                } else {
                    break;
                }
            }
            let token = match value.as_str() {
                "true" => Token::True,
                "false" => Token::False,
                "null" => Token::Null,
                "and" => Token::And,
                "or" => Token::Or,
                _ => Token::Identifier(value),
            };
            Ok(token)
        }
    }
    struct Parser {
        tokens: Vec<Token>,
        current: usize,
    }
    impl Parser {
        fn new(tokens: Vec<Token>) -> Self {
            Self { tokens, current: 0 }
        }
        fn parse(&mut self) -> Result<Expr, String> {
            self.conditional()
        }
        fn conditional(&mut self) -> Result<Expr, String> {
            let expr = self.or()?;
            if self.match_token(&Token::Question) {
                let then_expr = self.or()?;
                if !self.match_token(&Token::Colon) {
                    return Err("Expected ':' after '?' in conditional expression".to_string());
                }
                let else_expr = self.conditional()?;
                return Ok(Expr::Conditional {
                    condition: Box::new(expr),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                });
            }
            Ok(expr)
        }
        fn or(&mut self) -> Result<Expr, String> {
            let mut expr = self.and()?;
            while self.match_token(&Token::Or) {
                let op = self.previous().clone();
                let right = self.and()?;
                expr = Expr::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                };
            }
            Ok(expr)
        }
        fn and(&mut self) -> Result<Expr, String> {
            let mut expr = self.equality()?;
            while self.match_token(&Token::And) {
                let op = self.previous().clone();
                let right = self.equality()?;
                expr = Expr::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                };
            }
            Ok(expr)
        }
        fn equality(&mut self) -> Result<Expr, String> {
            let mut expr = self.comparison()?;
            while self.match_tokens(&[Token::BangEq, Token::EqEq]) {
                let op = self.previous().clone();
                let right = self.comparison()?;
                expr = Expr::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                };
            }
            Ok(expr)
        }
        fn comparison(&mut self) -> Result<Expr, String> {
            let mut expr = self.term()?;
            while self.match_tokens(&[Token::Gt, Token::GtEq, Token::Lt, Token::LtEq]) {
                let op = self.previous().clone();
                let right = self.term()?;
                expr = Expr::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                };
            }
            Ok(expr)
        }
        fn term(&mut self) -> Result<Expr, String> {
            let mut expr = self.factor()?;
            while self.match_tokens(&[Token::Minus, Token::Plus]) {
                let op = self.previous().clone();
                let right = self.factor()?;
                expr = Expr::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                };
            }
            Ok(expr)
        }
        fn factor(&mut self) -> Result<Expr, String> {
            let mut expr = self.unary()?;
            while self.match_tokens(&[Token::Slash, Token::Star, Token::Percent]) {
                let op = self.previous().clone();
                let right = self.unary()?;
                expr = Expr::Binary {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                };
            }
            Ok(expr)
        }
        fn unary(&mut self) -> Result<Expr, String> {
            if self.match_tokens(&[Token::Bang, Token::Minus]) {
                let op = self.previous().clone();
                let expr = self.unary()?;
                return Ok(Expr::Unary {
                    op,
                    expr: Box::new(expr),
                });
            }
            self.call()
        }
        fn call(&mut self) -> Result<Expr, String> {
            let mut expr = self.primary()?;
            loop {
                if self.match_token(&Token::LParen) {
                    expr = self.finish_call(expr)?;
                } else if self.match_token(&Token::LBracket) {
                    let index = self.or()?;
                    if !self.match_token(&Token::RBracket) {
                        return Err("Expected ']' after array index".to_string());
                    }
                    expr = Expr::Index {
                        object: Box::new(expr),
                        index: Box::new(index),
                    };
                } else {
                    break;
                }
            }
            Ok(expr)
        }
        fn finish_call(&mut self, callee: Expr) -> Result<Expr, String> {
            let mut args = Vec::new();
            if !self.check(&Token::RParen) {
                loop {
                    args.push(self.or()?);
                    if !self.match_token(&Token::Comma) {
                        break;
                    }
                }
            }
            if !self.match_token(&Token::RParen) {
                return Err("Expected ')' after function arguments".to_string());
            }
            Ok(Expr::Call {
                callee: Box::new(callee),
                args,
            })
        }
        fn primary(&mut self) -> Result<Expr, String> {
            if self.match_token(&Token::True) {
                return Ok(Expr::Literal(Value::Bool(true)));
            }
            if self.match_token(&Token::False) {
                return Ok(Expr::Literal(Value::Bool(false)));
            }
            if self.match_token(&Token::Null) {
                return Ok(Expr::Literal(Value::Null));
            }
            if let Token::Number(n) = self.peek() {
                let n_val = *n;
                self.advance();
                return Ok(Expr::Literal(Value::Number(
                    serde_json::Number::from_f64(n_val).unwrap(),
                )));
            }
            if let Token::String(s) = self.peek() {
                let s_val = s.clone();
                self.advance();
                return Ok(Expr::Literal(Value::String(s_val)));
            }
            if let Token::Identifier(_) = self.peek() {
                return self.parse_variable();
            }
            if self.match_token(&Token::LParen) {
                let expr = self.or()?;
                if !self.match_token(&Token::RParen) {
                    return Err("Expected ')' after expression".to_string());
                }
                return Ok(Expr::Grouping(Box::new(expr)));
            }
            Err("Expected expression".to_string())
        }
        fn parse_variable(&mut self) -> Result<Expr, String> {
            let mut path = Vec::new();
            if let Token::Identifier(name) = self.peek() {
                path.push(name.clone());
                self.advance();
            } else {
                return Err("Expected variable name".to_string());
            }
            while self.match_token(&Token::Dot) {
                if let Token::Identifier(name) = self.peek() {
                    path.push(name.clone());
                    self.advance();
                } else {
                    return Err("Expected property name after '.'".to_string());
                }
            }
            Ok(Expr::Variable(path))
        }
        fn match_token(&mut self, token_type: &Token) -> bool {
            if std::mem::discriminant(self.peek()) == std::mem::discriminant(token_type) {
                self.advance();
                true
            } else {
                false
            }
        }
        fn match_tokens(&mut self, types: &[Token]) -> bool {
            for token_type in types {
                if self.match_token(token_type) {
                    return true;
                }
            }
            false
        }
        fn peek(&self) -> &Token {
            self.tokens.get(self.current).unwrap_or(&Token::Eof)
        }
        fn previous(&self) -> &Token {
            &self.tokens[self.current - 1]
        }
        fn advance(&mut self) -> &Token {
            if !self.is_at_end() {
                self.current += 1;
            }
            self.previous()
        }
        fn is_at_end(&self) -> bool {
            matches!(self.peek(), Token::Eof)
        }
        fn check(&self, token_type: &Token) -> bool {
            std::mem::discriminant(self.peek()) == std::mem::discriminant(token_type)
        }
    }
    fn validate_ast(
        ast: &Expr,
        schema: &Value,
        block_id: &str,
        expression: &str,
    ) -> Result<(), TranspilerError> {
        match ast {
            Expr::Variable(path) => {
                let mut current = if let Some(properties) = schema.get("properties") {
                    properties
                } else {
                    schema
                };

                let path_to_check = if path.first() == Some(&"state".to_string()) {
                    &path[1..]
                } else {
                    path
                };

                for segment in path_to_check {
                    if let Some(next) = current.get(segment) {
                        current = next;
                    } else {
                        return Err(TranspilerError::StaticAnalysisError {
                            block_id: block_id.to_string(),
                            expr: expression.to_string(),
                            error: format!(
                                "Path 'state.{}' not found in schema.",
                                path_to_check.join(".")
                            ),
                        });
                    }
                }
            }
            Expr::Binary { left, right, .. } => {
                validate_ast(left, schema, block_id, expression)?;
                validate_ast(right, schema, block_id, expression)?;
            }
            Expr::Unary { expr, .. } => {
                validate_ast(expr, schema, block_id, expression)?;
            }
            Expr::Index { object, index } => {
                validate_ast(object, schema, block_id, expression)?;
                validate_ast(index, schema, block_id, expression)?;
            }
            Expr::Call { callee, args } => {
                validate_ast(callee, schema, block_id, expression)?;
                for arg in args {
                    validate_ast(arg, schema, block_id, expression)?;
                }
            }
            Expr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                validate_ast(condition, schema, block_id, expression)?;
                validate_ast(then_expr, schema, block_id, expression)?;
                validate_ast(else_expr, schema, block_id, expression)?;
            }
            Expr::Grouping(expr) => validate_ast(expr, schema, block_id, expression)?,
            Expr::Literal(_) => {}
        }
        Ok(())
    }
    fn compile_ast_to_bytecode(ast: &Expr) -> Result<Vec<u8>, String> {
        let mut assembler = BytecodeAssembler::new();
        compile_expr(ast, &mut assembler)?;
        Ok(assembler.into_bytecode())
    }

    fn compile_expr(expr: &Expr, assembler: &mut BytecodeAssembler) -> Result<(), String> {
        match expr {
            Expr::Literal(value) => {
                assembler.push_literal(value)
                    .map_err(|e| format!("Failed to compile literal: {e}"))?;
            }
            Expr::Variable(path) => {
                let filtered_path: Vec<&String> = if path.first() == Some(&"state".to_string()) {
                    path.iter().skip(1).collect()
                } else {
                    path.iter().collect()
                };
                let path_str = filtered_path
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                assembler.load_var(&path_str);
            }
            Expr::Binary { left, op, right } => {
                compile_expr(left, assembler)?;
                compile_expr(right, assembler)?;
                match op {
                    Token::Plus => { assembler.add(); }
                    Token::Minus => { assembler.subtract(); }
                    Token::Star => { assembler.multiply(); }
                    Token::Slash => { assembler.divide(); }
                    Token::Percent => { assembler.modulo(); }
                    Token::EqEq => { assembler.equal(); }
                    Token::BangEq => { assembler.not_equal(); }
                    Token::Gt => { assembler.greater_than(); }
                    Token::GtEq => { assembler.greater_equal(); }
                    Token::Lt => { assembler.less_than(); }
                    Token::LtEq => { assembler.less_equal(); }
                    Token::And => { assembler.and(); }
                    Token::Or => { assembler.or(); }
                    _ => return Err(format!("Unsupported binary operator: {op:?}")),
                };
            }
            Expr::Unary { op, expr } => {
                compile_expr(expr, assembler)?;
                match op {
                    Token::Minus => { assembler.negate(); }
                    Token::Bang => { assembler.not(); }
                    _ => return Err(format!("Unsupported unary operator: {op:?}")),
                };
            }
            Expr::Index { object, index } => {
                compile_expr(object, assembler)?;
                compile_expr(index, assembler)?;
                assembler.load_index();
            }
            Expr::Call { callee, args } => {
                for arg in args {
                    compile_expr(arg, assembler)?;
                }
                compile_expr(callee, assembler)?;
                assembler.call_function(args.len())
                    .map_err(|e| format!("Failed to compile function call: {e}"))?;
            }
            Expr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                compile_expr(condition, assembler)?;

                let else_jump_pos = assembler.jump_if_false();
                compile_expr(then_expr, assembler)?;

                let end_jump_pos = assembler.jump();
                assembler.patch_jump(else_jump_pos)
                    .map_err(|e| format!("Failed to patch else jump: {e}"))?;

                compile_expr(else_expr, assembler)?;
                assembler.patch_jump(end_jump_pos)
                    .map_err(|e| format!("Failed to patch end jump: {e}"))?;
            }
            Expr::Grouping(expr) => compile_expr(expr, assembler)?,
        }
        Ok(())
    }
}
#[derive(Error, Debug)]
pub enum TranspilerError {
    #[error("Invalid path format for key '{0}': {1}")]
    PathParseError(String, #[source] path_parser::PathParseError),
    #[error("Block with id '{0}' not found in flow definition")]
    BlockNotFound(String),
    #[error("A '{op}' block with ID '{block_id}' was used, but its target loop ID '{loop_id}' was not found.")]
    LoopJumpTargetNotFound {
        op: String,
        block_id: String,
        loop_id: String,
    },
    #[error("Duplicate loop ID '{0}' found. Loop IDs must be unique within a flow.")]
    DuplicateLoopId(String),
    #[error("Failed to convert initial state from JSON: {0}")]
    InitialStateConversionError(String),
    #[error("Static analysis failed for expression '{expr}' in block '{block_id}': {error}")]
    StaticAnalysisError {
        block_id: String,
        expr: String,
        error: String,
    },
    #[error("Expression parsing failed for '{expr}' in block '{block_id}': {error}")]
    ExpressionParseError {
        block_id: String,
        expr: String,
        error: String,
    },
    #[error("A TryCatch block '{0}' forms an invalid or inescapable structure.")]
    InvalidTryCatchStructure(String),
}
struct TranspilerContext {
    output_blocks: HashMap<String, AstNode>,
    internal_id_counter: u32,
    loop_continue_points: HashMap<String, String>,
    loop_break_points: HashMap<String, String>,
    try_scopes: HashMap<String, String>,
    try_exit_nodes: HashSet<String>,
}
impl TranspilerContext {
    fn new() -> Self {
        Self {
            output_blocks: HashMap::new(),
            internal_id_counter: 0,
            loop_continue_points: HashMap::new(),
            loop_break_points: HashMap::new(),
            try_scopes: HashMap::new(),
            try_exit_nodes: HashSet::new(),
        }
    }
    fn new_internal_id(&mut self, prefix: &str) -> String {
        let id = format!("__internal::{}_{}", prefix, self.internal_id_counter);
        self.internal_id_counter += 1;
        id
    }
}

struct ForEachParams<'a> {
    loop_id: &'a str,
    array_path: &'a str,
    iterator_var: &'a str,
    loop_body_block_id: &'a str,
    exit_block_id: &'a str,
}

pub struct FlowTranspiler;
impl FlowTranspiler {
    pub fn transpile(flow_def: &FlowDefinition) -> Result<Contract, TranspilerError> {
        let mut context = TranspilerContext::new();
        Self::collect_symbols_and_scopes(flow_def, &mut context)?;
        for block_def in &flow_def.blocks {
            Self::transpile_block(block_def, &mut context, flow_def)?;
        }
        let mut initial_state_node = match &flow_def.initial_state {
            Some(json_value) => AstNode::from(Op::Literal(
                Literal::try_from(json_value.clone())
                    .map_err(TranspilerError::InitialStateConversionError)?,
            )),
            None => AstNode::from(Op::Literal(Literal::Object(HashMap::new()))),
        };
        initial_state_node.metadata = create_source_map_meta("initial_state", "InitialState");
        Ok(Contract {
            version: "5.0.0-production".to_string(),
            initial_state: initial_state_node,
            start_block_id: flow_def.start_block_id.clone(),
            blocks: context.output_blocks,
            participants: flow_def.participants.clone(),
            permissions: flow_def.permissions.clone(),
        })
    }
    fn collect_symbols_and_scopes(
        flow_def: &FlowDefinition,
        context: &mut TranspilerContext,
    ) -> Result<(), TranspilerError> {
        let block_map: HashMap<_, _> = flow_def.blocks.iter().map(|b| (b.id.as_str(), b)).collect();
        for block_def in &flow_def.blocks {
            match &block_def.block_type {
                BlockType::ForEach {
                    loop_id,
                    exit_block_id,
                    ..
                } => {
                    if context.loop_continue_points.contains_key(loop_id) {
                        return Err(TranspilerError::DuplicateLoopId(loop_id.clone()));
                    }
                    let increment_block_id =
                        context.new_internal_id(&format!("increment_{loop_id}"));
                    context
                        .loop_continue_points
                        .insert(loop_id.clone(), increment_block_id);
                    context
                        .loop_break_points
                        .insert(loop_id.clone(), exit_block_id.clone());
                }
                BlockType::TryCatch {
                    try_block_id,
                    catch_block_id,
                } => {
                    let mut queue = VecDeque::new();
                    let mut visited = HashSet::new();
                    queue.push_back(try_block_id.clone());
                    visited.insert(try_block_id.clone());
                    while let Some(current_id) = queue.pop_front() {
                        context
                            .try_scopes
                            .insert(current_id.clone(), catch_block_id.clone());
                        let current_block = block_map
                            .get(current_id.as_str())
                            .ok_or_else(|| TranspilerError::BlockNotFound(current_id.clone()))?;
                        for successor_id in get_successor_ids(current_block) {
                            if successor_id == *catch_block_id {
                                continue;
                            }
                            if let Some(successor_block) = block_map.get(successor_id.as_str()) {
                                let is_outside_scope = get_successor_ids(successor_block)
                                    .iter()
                                    .any(|s| !visited.contains(s));
                                if is_outside_scope {
                                    context.try_exit_nodes.insert(current_id.clone());
                                }
                                if visited.insert(successor_id.clone()) {
                                    queue.push_back(successor_id);
                                }
                            } else {
                                context.try_exit_nodes.insert(current_id.clone());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn transpile_block(
        block_def: &BlockDefinition,
        context: &mut TranspilerContext,
        flow_def: &FlowDefinition,
    ) -> Result<(), TranspilerError> {
        let meta = create_source_map_meta(&block_def.id, &format!("{:?}", block_def.block_type));
        let mut node = match &block_def.block_type {
            BlockType::Conditional {
                condition,
                true_block,
                false_block,
            } => {
                let bytecode = expression_compiler::compile(condition, flow_def, &block_def.id)?;
                let temp_output_path =
                    vec![PathSegment::Key(context.new_internal_id("cond_result"))];
                AstNode::from(Op::If {
                    condition: Box::new(AstNode::from(Op::Evaluate {
                        bytecode,
                        output_path: temp_output_path,
                    })),
                    then_branch: Box::new(AstNode::from(Op::SetNextBlock(true_block.clone()))),
                    else_branch: Some(Box::new(AstNode::from(Op::SetNextBlock(
                        false_block.clone(),
                    )))),
                })
            }
            BlockType::Compute {
                expression,
                output_key,
                next_block,
            } => {
                let bytecode = expression_compiler::compile(expression, flow_def, &block_def.id)?;
                let mut path = path_parser::transpile(output_key)
                    .map_err(|e| TranspilerError::PathParseError(output_key.clone(), e))?;
                path.insert(0, PathSegment::State);
                AstNode::from(Op::Sequence(vec![
                    AstNode::from(Op::Evaluate {
                        bytecode,
                        output_path: path,
                    }),
                    AstNode::from(Op::SetNextBlock(next_block.clone())),
                ]))
            }
            BlockType::AwaitInput {
                interaction_id,
                agent_id,
                prompt,
                state_key,
                next_block,
            } => {
                let bytecode = expression_compiler::compile(prompt, flow_def, &block_def.id)?;
                let prompt_node = AstNode::from(Op::Evaluate {
                    bytecode,
                    output_path: vec![PathSegment::Key(context.new_internal_id("prompt_result"))],
                });
                let mut path = path_parser::transpile(state_key)
                    .map_err(|e| TranspilerError::PathParseError(state_key.clone(), e))?;
                path.insert(0, PathSegment::State);

                let mut await_node = AstNode::from(Op::Await {
                    interaction_id: interaction_id.clone(),
                    agent_id: agent_id.clone(),
                    prompt: Some(Box::new(prompt_node)),
                    timeout_ms: None,
                });

                await_node
                    .metadata
                    .insert("next_block".to_string(), json!(next_block));
                await_node
                    .metadata
                    .insert("state_key".to_string(), json!(state_key));

                AstNode::from(Op::Sequence(vec![await_node]))
            }
            BlockType::Terminate => AstNode::from(Op::Terminate),
            BlockType::ForEach {
                loop_id,
                array_path,
                iterator_var,
                loop_body_block_id,
                exit_block_id,
            } => {
                return Self::transpile_for_each(
                    block_def,
                    ForEachParams {
                        loop_id,
                        array_path,
                        iterator_var,
                        loop_body_block_id,
                        exit_block_id,
                    },
                    context,
                    flow_def,
                );
            }
            BlockType::Continue { loop_id } => AstNode::from(Op::SetNextBlock(
                context
                    .loop_continue_points
                    .get(loop_id)
                    .ok_or_else(|| TranspilerError::LoopJumpTargetNotFound {
                        op: "Continue".into(),
                        block_id: block_def.id.clone(),
                        loop_id: loop_id.clone(),
                    })?
                    .clone(),
            )),
            BlockType::Break { loop_id } => AstNode::from(Op::SetNextBlock(
                context
                    .loop_break_points
                    .get(loop_id)
                    .ok_or_else(|| TranspilerError::LoopJumpTargetNotFound {
                        op: "Break".into(),
                        block_id: block_def.id.clone(),
                        loop_id: loop_id.clone(),
                    })?
                    .clone(),
            )),
            BlockType::TryCatch {
                try_block_id,
                catch_block_id,
            } => AstNode::from(Op::Sequence(vec![
                AstNode::from(Op::PushErrorHandler {
                    catch_block_id: catch_block_id.clone(),
                }),
                AstNode::from(Op::SetNextBlock(try_block_id.clone())),
            ])),
            BlockType::SubFlow { next_block, .. } => {
                AstNode::from(Op::SetNextBlock(next_block.clone()))
            }
        };
        if context.try_exit_nodes.contains(&block_def.id) {
            node = Self::inject_pop_handler(node);
        }
        node.metadata = meta;
        context.output_blocks.insert(block_def.id.clone(), node);
        Ok(())
    }
    fn inject_pop_handler(mut node: AstNode) -> AstNode {
        match node.op {
            Op::Sequence(ref mut ops) => {
                if let Some(last_op) = ops.last_mut() {
                    if let Op::SetNextBlock(_) = last_op.op {
                        let set_next = ops.pop().unwrap();
                        ops.push(AstNode::from(Op::PopErrorHandler));
                        ops.push(set_next);
                    }
                }
            }
            Op::If {
                ref mut then_branch,
                ref mut else_branch,
                ..
            } => {
                *then_branch = Box::new(Self::inject_pop_handler(*then_branch.clone()));
                if let Some(eb) = else_branch {
                    *eb = Box::new(Self::inject_pop_handler(*eb.clone()));
                }
            }
            _ => {}
        }
        node
    }

    fn transpile_for_each(
        block_def: &BlockDefinition,
        params: ForEachParams,
        context: &mut TranspilerContext,
        flow_def: &FlowDefinition,
    ) -> Result<(), TranspilerError> {
        let meta = create_source_map_meta(&block_def.id, "ForEach");
        let temp_array_key = context.new_internal_id(&format!("array_{}", params.loop_id));
        let counter_key = context.new_internal_id(&format!("counter_{}", params.loop_id));
        let init_block_id = context.new_internal_id(&format!("init_{}", params.loop_id));
        let cond_block_id = context.new_internal_id(&format!("cond_{}", params.loop_id));
        let body_setup_block_id =
            context.new_internal_id(&format!("body_setup_{}", params.loop_id));
        let increment_block_id = context
            .loop_continue_points
            .get(params.loop_id)
            .unwrap()
            .clone();
        let bytecode = expression_compiler::compile(params.array_path, flow_def, &block_def.id)?;
        let mut pre_loop_block = AstNode::from(Op::Sequence(vec![
            AstNode::from(Op::Evaluate {
                bytecode,
                output_path: vec![PathSegment::State, PathSegment::Key(temp_array_key.clone())],
            }),
            AstNode::from(Op::SetNextBlock(init_block_id.clone())),
        ]));
        pre_loop_block.metadata = meta.clone();
        context
            .output_blocks
            .insert(block_def.id.clone(), pre_loop_block);
        let mut init_block = AstNode::from(Op::Sequence(vec![
            AstNode::from(Op::Assign {
                path: vec![PathSegment::State, PathSegment::Key(counter_key.clone())],
                value: Box::new(AstNode::from(Op::Literal(Literal::Number(0.0)))),
            }),
            AstNode::from(Op::SetNextBlock(cond_block_id.clone())),
        ]));
        init_block.metadata = meta.clone();
        context.output_blocks.insert(init_block_id, init_block);
        let fetch_counter = AstNode::from(Op::Fetch(vec![
            PathSegment::State,
            PathSegment::Key(counter_key.clone()),
        ]));
        let fetch_array = AstNode::from(Op::Fetch(vec![
            PathSegment::State,
            PathSegment::Key(temp_array_key.clone()),
        ]));
        let condition = AstNode::from(Op::LessThan(
            Box::new(fetch_counter.clone()),
            Box::new(AstNode::from(Op::Length(Box::new(fetch_array.clone())))),
        ));
        let mut cond_block = AstNode::from(Op::If {
            condition: Box::new(condition),
            then_branch: Box::new(AstNode::from(Op::SetNextBlock(body_setup_block_id.clone()))),
            else_branch: Some(Box::new(AstNode::from(Op::SetNextBlock(
                params.exit_block_id.to_string(),
            )))),
        });
        cond_block.metadata = meta.clone();
        context
            .output_blocks
            .insert(cond_block_id.clone(), cond_block);
        let mut body_setup_block = AstNode::from(Op::Sequence(vec![
            AstNode::from(Op::Assign {
                path: vec![
                    PathSegment::State,
                    PathSegment::Key(params.iterator_var.to_string()),
                ],
                value: Box::new(AstNode::from(Op::Fetch(vec![
                    PathSegment::State,
                    PathSegment::Key(temp_array_key.clone()),
                    PathSegment::DynamicOffset(Box::new(fetch_counter.clone())),
                ]))),
            }),
            AstNode::from(Op::SetNextBlock(params.loop_body_block_id.to_string())),
        ]));
        body_setup_block.metadata = meta.clone();
        context
            .output_blocks
            .insert(body_setup_block_id, body_setup_block);
        let mut increment_block = AstNode::from(Op::Sequence(vec![
            AstNode::from(Op::Assign {
                path: vec![PathSegment::State, PathSegment::Key(counter_key.clone())],
                value: Box::new(AstNode::from(Op::Add(
                    Box::new(fetch_counter),
                    Box::new(AstNode::from(Op::Literal(Literal::Number(1.0)))),
                ))),
            }),
            AstNode::from(Op::SetNextBlock(cond_block_id.clone())),
        ]));
        increment_block.metadata = meta;
        context
            .output_blocks
            .insert(increment_block_id, increment_block);
        Ok(())
    }
}
fn get_successor_ids(block_def: &BlockDefinition) -> Vec<String> {
    match &block_def.block_type {
        BlockType::Conditional {
            true_block,
            false_block,
            ..
        } => vec![true_block.clone(), false_block.clone()],
        BlockType::Compute { next_block, .. } | BlockType::AwaitInput { next_block, .. } => {
            vec![next_block.clone()]
        }
        BlockType::ForEach {
            loop_body_block_id,
            exit_block_id,
            ..
        } => vec![loop_body_block_id.clone(), exit_block_id.clone()],
        BlockType::TryCatch { try_block_id, .. } => vec![try_block_id.clone()],
        BlockType::SubFlow { next_block, .. } => vec![next_block.clone()],
        BlockType::Continue { .. } | BlockType::Break { .. } | BlockType::Terminate => vec![],
    }
}
fn create_source_map_meta(
    source_block_id: &str,
    source_block_type: &str,
) -> HashMap<String, Value> {
    [
        ("source_block_id".to_string(), json!(source_block_id)),
        ("source_block_type".to_string(), json!(source_block_type)),
    ]
    .into_iter()
    .collect()
}
