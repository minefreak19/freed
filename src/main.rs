use std::{env, fmt, str};

#[derive(Debug)]
enum Keyword {
    Stream,
    EndStream,
    Obj,
    EndObj,
    Reference,
    Xref,
    Trailer,
    InUseEntry,
    FreeEntry,
    StartXref,
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Keyword::Stream => {
                write!(f, "stream")
            }
            Keyword::EndStream => {
                write!(f, "endstream")
            }
            Keyword::Obj => {
                write!(f, "obj")
            }
            Keyword::EndObj => {
                write!(f, "endobj")
            }
            Keyword::Reference => {
                write!(f, "R")
            }
            Keyword::Xref => {
                write!(f, "xref")
            }
            Keyword::Trailer => {
                write!(f, "trailer")
            }
            Keyword::InUseEntry => {
                write!(f, "n")
            }
            Keyword::FreeEntry => {
                write!(f, "f")
            }
            Keyword::StartXref => {
                write!(f, "startxref")
            }
        }
    }
}

enum Token {
    // String version
    Header(String),
    // String comment
    Comment(String),
    Boolean(bool),
    Numeric(f64),
    // Vec<u8> to account for hexadecimal strings
    // This is also emitted for binary data in a stream. In this case, it stores the (encoded) data
    // as found in the file. Decoding is the job of the Parser.
    String(Vec<u8>),
    Name(Vec<u8>),
    Keyword(Keyword),
    // This goes here instead of as a keyword as there's a semantic difference, it has semantic
    // (and not structural significance, and it should be considered a special entity when it comes
    // to elements of dictionaries.
    Null,
    ArrayBegin,
    ArrayEnd,
    DictBegin,
    DictEnd,
    EOF,
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Header(version) => {
                write!(f, "PDF Header [version {version}]")
            }
            Token::Comment(comment) => {
                write!(f, "Comment `{comment}`")
            }
            Token::Boolean(b) => {
                write!(f, "Boolean {}", b)
            }
            Token::Numeric(num) => {
                write!(f, "Number {num}")
            }
            // TODO: Print tokens properly without cloning data
            // This might be necessary in the future in case larger images are included
            Token::String(data) => match String::from_utf8(data.clone()) {
                Ok(s) => {
                    write!(f, "String (UTF-8): `{s}`")
                }
                Err(_) => {
                    write!(f, "String (arbitrary): `{data:#02X?}`")
                }
            },
            Token::Name(name) => match String::from_utf8(name.clone()) {
                Ok(s) => {
                    write!(f, "Name (UTF-8): `{s}`")
                }
                Err(_) => {
                    write!(f, "Name (arbitrary): `{name:#02X?}`")
                }
            },
            Token::Keyword(kw) => {
                write!(f, "Keyword `{}`", kw)
            }
            Token::Null => {
                write!(f, "Null")
            }
            Token::ArrayBegin => {
                write!(f, "Begin Array")
            }
            Token::ArrayEnd => {
                write!(f, "End Array")
            }
            Token::DictBegin => {
                write!(f, "Begin Dict")
            }
            Token::DictEnd => {
                write!(f, "End Dict")
            }
            Token::EOF => {
                write!(f, "[EOF Marker]")
            }
        }
    }
}

#[derive(PartialEq)]
enum LexerState {
    Skip,
    Lex,
    Stream,
}

struct Lexer {
    data: Vec<u8>,
    cur: usize,
    state: LexerState,
}

impl Lexer {
    fn new(data: Vec<u8>) -> Self {
        Self {
            cur: 0,
            data,
            state: LexerState::Skip,
        }
    }

    fn is_whitespace(ch: u8) -> bool {
        matches!(ch, b'\0' | b'\t' | b'\n' | b'\x0C' | b'\r' | b' ')
    }

    fn is_delim(ch: u8) -> bool {
        matches!(
            ch,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
    }

    fn is_normal(ch: u8) -> bool {
        !Self::is_delim(ch) && !Self::is_whitespace(ch)
    }

    fn is_num(ch: u8) -> bool {
        matches!(ch, b'+' | b'-' | b'.' | b'0'..=b'9')
    }

    fn chop_char(&mut self) -> Option<u8> {
        self.cur += 1;
        self.data.get(self.cur - 1).cloned().map(|ch| {
            if ch == b'\r' {
                if let Some(x) = self.data.get(self.cur).cloned() {
                    if x == b'\n' {
                        self.cur += 1;
                    }
                }
                return b'\n';
            }
            ch
        })
    }

    fn chop_n_chars(&mut self, n: usize) -> Option<Vec<u8>> {
        let mut ret = vec![];
        for _ in 0..n {
            ret.push(self.chop_char()?);
        }
        Some(ret)
    }

    // TODO: Add Lexer::drop_while
    fn chop_while(&mut self, predicate: fn(u8) -> bool) -> Option<Vec<u8>> {
        let mut ret = vec![];
        while predicate(*self.data.get(self.cur)?) {
            ret.push(self.chop_char()?);
        }
        Some(ret)
    }

    fn chop_line(&mut self) -> Option<Vec<u8>> {
        if self.cur >= self.data.len() {
            return None;
        }

        let mut ret = vec![];
        while let Some(x) = self.chop_char() {
            if x == b'\n' {
                return Some(ret);
            }
            ret.push(x);
        }
        Some(ret)
    }

    fn starts_with(&self, s: &[u8]) -> bool {
        if self.data.len() - self.cur - 1 < s.len() {
            return false;
        }
        for i in 0..s.len() {
            if self.data[self.cur + i] != s[i] {
                return false;
            }
        }
        true
    }

    fn next_token(&mut self) -> Option<Token> {
        if self.state == LexerState::Stream {
            if self.cur == self.data.len() {
                return None;
            }

            // An assertion, not a check, because something has gone really wrong if cur > data.len
            assert!(self.cur < self.data.len());

            // self.chop_char() isn't used here because it messes with newline chars
            // Binary data should be preserved perfectly.
            assert!(self.chop_char()? == b'\n');

            let mut result = vec![];
            loop {
                if self.starts_with(b"\r\nendstream") || self.starts_with(b"\nendstream") {
                    self.chop_char();
                    break;
                }
                if self.starts_with(b"endstream") {
                    break;
                }

                result.push(self.data[self.cur]);
                self.cur += 1;
            }
            self.state = LexerState::Lex;
            return Some(Token::String(result));
        }

        self.chop_while(Self::is_whitespace);

        if self.cur == self.data.len() {
            return None;
        }

        // An assertion, not a check, because something has gone really wrong if cur > data.len
        assert!(self.cur < self.data.len());

        if self.state == LexerState::Skip {
            while !self.starts_with(b"%PDF-") {
                self.chop_char()?;
            }

            let header = self.chop_line().expect("Not a PDF file");
            let mut version = String::new();
            for x in &header[5..=7] {
                version.push(*x as char);
            }
            self.state = LexerState::Lex;
            return Some(Token::Header(version));
        }

        match self.data.get(self.cur)? {
            b'%' => {
                self.cur += 1;
                let line = self.chop_line().unwrap_or(vec![]);
                let mut comment = String::new();
                for x in line {
                    comment.push(x as char);
                }
                if comment == "%EOF" {
                    return Some(Token::EOF);
                }
                return Some(Token::Comment(comment));
            }

            b'+' | b'-' | b'.' | b'0'..=b'9' => {
                let mut s = String::new();
                while Self::is_num(*self.data.get(self.cur)?) {
                    s.push(self.chop_char()? as char);
                }
                let num = str::parse::<f64>(&s)
                    .map_err(|_| {
                        panic!(
                            "ERROR: index {}: Could not parse numeric string `{s}`",
                            self.cur
                        );
                    })
                    .ok()?;
                return Some(Token::Numeric(num));
            }

            b'(' => {
                self.chop_char();
                return self.literal_string_token();
            }

            b'<' => {
                self.chop_char();
                if *self.data.get(self.cur)? == b'<' {
                    self.chop_char();
                    return Some(Token::DictBegin);
                } else {
                    return self.hex_string_token();
                }
            }

            b'>' => {
                if let Some(b'>') = self.data.get(self.cur + 1) {
                    self.chop_n_chars(2);
                    return Some(Token::DictEnd);
                } else {
                    panic!("ERROR: index {}: Stray '>' in document", self.cur);
                }
            }

            b'/' => {
                self.chop_char();
                return self.name_token();
            }

            b'[' => {
                self.chop_char();
                return Some(Token::ArrayBegin);
            }

            b']' => {
                self.chop_char();
                return Some(Token::ArrayEnd);
            }

            _ => {
                let word = self
                    .chop_while(Self::is_normal)
                    .unwrap_or(self.data[self.cur..].to_owned());
                return match word.as_slice() {
                    b"true" => Some(Token::Boolean(true)),
                    b"false" => Some(Token::Boolean(false)),

                    b"null" => Some(Token::Null),

                    b"stream" => {
                        self.state = LexerState::Stream;
                        Some(Token::Keyword(Keyword::Stream))
                    }
                    b"endstream" => Some(Token::Keyword(Keyword::EndStream)),

                    b"obj" => Some(Token::Keyword(Keyword::Obj)),
                    b"endobj" => Some(Token::Keyword(Keyword::EndObj)),

                    b"R" => Some(Token::Keyword(Keyword::Reference)),

                    b"xref" => Some(Token::Keyword(Keyword::Xref)),
                    b"trailer" => Some(Token::Keyword(Keyword::Trailer)),
                    b"n" => Some(Token::Keyword(Keyword::InUseEntry)),
                    b"f" => Some(Token::Keyword(Keyword::FreeEntry)),
                    b"startxref" => Some(Token::Keyword(Keyword::StartXref)),

                    _ => {
                        panic!(
                            "UNIMPLEMENTED: unknown word `{}` (index {})",
                            String::from_utf8(word.clone())
                                .unwrap_or_else(|_| { format!("[non-utf-8: {:?}]", word.clone()) }),
                            self.cur
                        );
                    }
                };
            }
        }
    }

    fn literal_string_token(&mut self) -> Option<Token> {
        let mut result = vec![];
        // level of parens
        // incremented for a left paren, decremented for a right paren
        let mut level = 1;

        while level > 0 {
            match self.data.get(self.cur)? {
                b'(' => {
                    level += 1;
                    result.push(self.chop_char()?);
                }

                b')' => {
                    level -= 1;
                    if level != 0 {
                        result.push(self.chop_char()?);
                    } else {
                        self.chop_char();
                    }
                }

                b'\\' => {
                    self.chop_char();
                    match self.data.get(self.cur)? {
                        b'n' => {
                            self.chop_char();
                            result.push(b'\n');
                        }
                        b'r' => {
                            self.chop_char();
                            result.push(b'\r');
                        }
                        b't' => {
                            self.chop_char();
                            result.push(b'\t');
                        }
                        b'b' => {
                            self.chop_char();
                            result.push(0x08_u8);
                        }
                        b'f' => {
                            self.chop_char();
                            result.push(0x0C_u8);
                        }
                        b'(' => {
                            self.chop_char();
                            result.push(b'(');
                        }
                        b')' => {
                            self.chop_char();
                            result.push(b')');
                        }
                        b'\\' => {
                            self.chop_char();
                            result.push(b'\\');
                        }

                        b'\n' => {
                            self.chop_char();
                        }

                        b'0'..=b'7' => {
                            let mut s = String::with_capacity(3);
                            let mut i = 0;
                            while i < 3 && (b'0'..=b'7').contains(self.data.get(self.cur)?) {
                                s.push(self.chop_char()? as char);
                                i += 1;
                            }
                            // Already made sure everything is '0'..='7' in the loop
                            result.push(u8::from_str_radix(&s, 8).unwrap());
                        }

                        _ => {
                            panic!(
                                "ERROR: index {}: Invalid escape character `{}`",
                                self.cur, self.data[self.cur] as char
                            );
                        }
                    }
                }

                _ => {
                    result.push(self.chop_char()?);
                }
            }
        }

        return Some(Token::String(result));
    }

    // This code allocates 3x the length of the data (ie for <AABBCC> it allocates 9 bytes of
    // memory
    fn hex_string_token(&mut self) -> Option<Token> {
        // TODO: Lex hex string tokens without excess memory allocation
        fn make_pairs(v: Vec<u8>) -> Vec<u8> {
            assert!(v.len() % 2 == 0);
            let mut result = Vec::with_capacity(v.len() / 2);
            for i in (0..v.len() / 2).map(|x| x * 2) {
                result.push((v[i] << 4) | (v[i + 1]));
            }
            result
        }

        let mut result: Vec<u8> = vec![];
        loop {
            match self.data.get(self.cur)? {
                b'0'..=b'9' => {
                    result.push(self.chop_char().unwrap() - b'0');
                }

                b'a'..=b'f' => {
                    result.push(self.chop_char().unwrap() - b'a' + 10);
                }

                b'A'..=b'F' => {
                    result.push(self.chop_char().unwrap() - b'A' + 10);
                }

                b'>' => {
                    self.chop_char();
                    break;
                }

                _ => {
                    if Self::is_whitespace(self.data[self.cur]) {
                        self.chop_char();
                        continue;
                    } else {
                        panic!(
                            "ERROR: index {}: Illegal character `{}` in hex literal",
                            self.cur, self.data[self.cur]
                        );
                    }
                }
            }
        }

        if result.len() % 2 == 1 {
            result.push(0);
        }

        result = make_pairs(result);

        return Some(Token::String(result));
    }

    fn name_token(&mut self) -> Option<Token> {
        let mut result = vec![];
        while Self::is_normal(*self.data.get(self.cur)?) {
            match self.data[self.cur] {
                b'#' => {
                    self.chop_char();
                    let hex_code = self.chop_n_chars(2)?;
                    let str = str::from_utf8(&hex_code).expect("Valid UTF-8 in hex escape in name");
                    let x = u8::from_str_radix(str, 16)
                        .map_err(|_| {
                            panic!("ERROR: index {}: Illegal hex code {}", self.cur, str);
                        })
                        .unwrap();
                    result.push(x);
                }

                _ => {
                    result.push(self.chop_char()?);
                }
            }
        }
        return Some(Token::Name(result));
    }
}

fn main() -> Result<(), ()> {
    let args: Vec<_> = env::args().collect();
    let path = if args.len() > 1 {
        &args[1]
    } else {
        "./test.pdf"
    };

    let data = std::fs::read(path).map_err(|e| {
        eprintln!("ERROR: Could not read file {path}: {e}");
    })?;

    let mut lexer = Lexer::new(data);
    while let Some(token) = lexer.next_token() {
        println!("{:?}", token);
    }

    Ok(())
}
