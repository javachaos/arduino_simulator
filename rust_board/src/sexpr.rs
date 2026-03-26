use crate::dsl::DslError;

#[derive(Debug, Clone, PartialEq)]
pub enum SExpr {
    Atom(String),
    List(Vec<SExpr>),
}

impl SExpr {
    pub fn as_atom(&self) -> Option<&str> {
        match self {
            Self::Atom(value) => Some(value),
            Self::List(_) => None,
        }
    }

    pub fn as_list(&self) -> Option<&[SExpr]> {
        match self {
            Self::Atom(_) => None,
            Self::List(values) => Some(values),
        }
    }
}

#[derive(Debug)]
struct TokenStream {
    tokens: Vec<String>,
    index: usize,
}

impl TokenStream {
    fn new(tokens: Vec<String>) -> Self {
        Self { tokens, index: 0 }
    }

    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.index).map(String::as_str)
    }

    fn pop(&mut self) -> Result<String, DslError> {
        let token = self
            .tokens
            .get(self.index)
            .cloned()
            .ok_or_else(|| DslError::new("unexpected end of input"))?;
        self.index += 1;
        Ok(token)
    }
}

fn tokenize(text: &str) -> Result<Vec<String>, DslError> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        let character = chars[index];

        if character.is_whitespace() {
            index += 1;
            continue;
        }

        if character == ';' {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }

        if character == '(' || character == ')' {
            tokens.push(character.to_string());
            index += 1;
            continue;
        }

        if character == '"' {
            index += 1;
            let mut value = String::new();
            while index < chars.len() {
                let current = chars[index];
                if current == '\\' && (index + 1) < chars.len() {
                    value.push(chars[index + 1]);
                    index += 2;
                    continue;
                }
                if current == '"' {
                    index += 1;
                    break;
                }
                value.push(current);
                index += 1;
            }
            if index > chars.len() {
                return Err(DslError::new("unterminated string in S-expression"));
            }
            tokens.push(value);
            continue;
        }

        let start = index;
        while index < chars.len()
            && !chars[index].is_whitespace()
            && chars[index] != '('
            && chars[index] != ')'
        {
            index += 1;
        }
        tokens.push(chars[start..index].iter().collect());
    }

    Ok(tokens)
}

fn parse_expr(stream: &mut TokenStream) -> Result<SExpr, DslError> {
    let token = stream.pop()?;

    if token == "(" {
        let mut values = Vec::new();
        loop {
            let Some(next_token) = stream.peek() else {
                return Err(DslError::new("unterminated list in S-expression"));
            };
            if next_token == ")" {
                let _ = stream.pop()?;
                return Ok(SExpr::List(values));
            }
            values.push(parse_expr(stream)?);
        }
    }

    if token == ")" {
        return Err(DslError::new("unexpected ')' in S-expression"));
    }

    Ok(SExpr::Atom(token))
}

pub fn parse_sexpr(text: &str) -> Result<SExpr, DslError> {
    let mut stream = TokenStream::new(tokenize(text)?);
    let result = parse_expr(&mut stream)?;
    if stream.peek().is_some() {
        return Err(DslError::new("extra tokens after root S-expression"));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{parse_sexpr, SExpr};

    #[test]
    fn parse_basic_list() {
        let parsed = parse_sexpr("(root (child 1) \"text\")").expect("parse");
        assert_eq!(
            parsed,
            SExpr::List(vec![
                SExpr::Atom("root".to_string()),
                SExpr::List(vec![
                    SExpr::Atom("child".to_string()),
                    SExpr::Atom("1".to_string())
                ]),
                SExpr::Atom("text".to_string())
            ])
        );
    }

    #[test]
    fn parse_skips_comments() {
        let parsed = parse_sexpr("(root ; comment\n child)").expect("parse");
        assert!(matches!(parsed, SExpr::List(_)));
    }
}
