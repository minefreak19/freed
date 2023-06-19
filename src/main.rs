use core::fmt;
use std::{
    cmp::Ordering,
    collections::HashMap,
    env, fs,
    str::{self, FromStr},
};

#[derive(Clone, Debug, PartialEq)]
struct Version(u8, u8);

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let Self(smaj, smin) = self;
        let Self(omaj, omin) = other;
        if smaj == omaj {
            if smin == omin {
                Some(Ordering::Equal)
            } else if smin > omin {
                Some(Ordering::Greater)
            } else {
                Some(Ordering::Less)
            }
        } else if smaj > omaj {
            Some(Ordering::Greater)
        } else {
            Some(Ordering::Less)
        }
    }
}

#[derive(Debug, PartialEq)]
enum Keyword {
    R,
    Xref,
    EntryInUse,
    EntryFree,
    Obj,
    EndObj,
    Stream,
    EndStream,
}

impl FromStr for Keyword {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "R" => Ok(Keyword::R),
            "xref" => Ok(Keyword::Xref),
            "n" => Ok(Keyword::EntryInUse),
            "f" => Ok(Keyword::EntryFree),

            "obj" => Ok(Keyword::Obj),
            "endobj" => Ok(Keyword::EndObj),

            "stream" => Ok(Keyword::Stream),
            "endstream" => Ok(Keyword::EndStream),
            _ => Err(()),
        }
    }
}

#[derive(PartialEq)]
enum Token {
    ArrayBegin,
    ArrayEnd,

    DictBegin,
    DictEnd,

    Solidus,

    Float(f64),
    Int(i64),

    String(Vec<u8>),

    Keyword(Keyword),
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::ArrayBegin => write!(f, "ArrayBegin"),
            Token::ArrayEnd => write!(f, "ArrayEnd"),
            Token::DictBegin => write!(f, "DictBegin"),
            Token::DictEnd => write!(f, "DictEnd"),
            Token::Solidus => write!(f, "Solidus"),

            Token::Float(fl) => {
                write!(f, "Float({})", fl)
            }
            Token::Int(i) => {
                write!(f, "Int({})", i)
            }

            Token::String(data) => match String::from_utf8(data.clone()) {
                Ok(s) => {
                    write!(f, "String(`{s}`)")
                }
                Err(_) => {
                    write!(f, "String({data:#02X?})")
                }
            },

            Token::Keyword(kw) => {
                write!(f, "Keyword({:?})", kw)
            }
        }
    }
}

#[derive(Clone)]
enum Object<'a> {
    Int(i64),
    Float(f64),
    String(Vec<u8>),
    Name(&'a str),
    Array(Vec<Object<'a>>),
    Dict(HashMap<&'a str, Object<'a>>),
    Stream(HashMap<&'a str, Object<'a>>, &'a [u8]),
    // refnum, gennum
    RawReference(i64, i64),
}

impl fmt::Debug for Object<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Object::Int(i) => write!(f, "Int({})", i),
            Object::Float(fl) => write!(f, "Float({})", fl),
            Object::String(s) => match String::from_utf8(s.clone()) {
                Ok(string) => {
                    write!(f, "String(`{}`)", string)
                }
                Err(_) => {
                    write!(f, "String({:#02X?})", s)
                }
            },
            Object::Name(name) => write!(f, "Name(`{}`)", name),
            Object::Array(arr) => write!(f, "Array({:#?})", arr),
            Object::Dict(dict) => write!(f, "Dict({:#?})", dict),
            Object::Stream(dict, data) => write!(f, "Stream(dict: {:#?}, data: {:#?})", dict, data),
            Object::RawReference(refnum, gennum) => {
                write!(f, "RawReference({}, {})", refnum, gennum)
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Parser<'a> {
    data: &'a [u8],
    start: usize,
    end: usize,
    cur: usize,

    version: Version,

    trailer_dict: HashMap<&'a str, Object<'a>>,
    xref_table: HashMap<usize, Object<'a>>,
}

impl<'a> Parser<'a> {
    fn new(data: &'a [u8]) -> Self {
        let mut ret = Self {
            start: 0,
            end: 0,
            cur: 0,

            version: Version(0, 0),
            data,

            trailer_dict: HashMap::new(),
            xref_table: HashMap::new(),
        };

        ret.init();
        ret
    }

    fn init(&mut self) {
        // Set start, version
        while !self.data[self.cur..].starts_with(b"%PDF-") {
            self.cur += 1;
        }
        self.start = self.cur;
        self.cur += 5;
        let vmaj = self
            .chop_int::<u8>()
            .expect("`%PDF-` must be followed by version number");
        assert_eq!(self.chop_char(), Some(b'.'));
        let vmin = self
            .chop_int::<u8>()
            .expect("`%PDF-` must be followed by version number");
        self.version = Version(vmaj, vmin);

        // TODO: is Parser::end necessary?
        // Set end
        self.cur = self.data.len() - 1;
        while !self.data[self.cur + 1..].starts_with(b"%%EOF") {
            self.cur -= 1;
        }
        self.end = self.cur + 1;
        if !matches!(self.chop_char_backwards(), Some(b'\n')) {
            panic!("index {}: expected newline before EOF marker", self.cur);
        }

        if self.version > Version(1, 4) {
            panic!("TODO: Versions after PDF 1.4 are not supported");
        }

        // Get xref table offset
        while self.data[self.cur - 1].is_ascii_digit() {
            self.chop_char_backwards();
        }
        let xref_offset = self
            .chop_int::<usize>()
            .expect("Offset to Xref table must be located immediately before %%EOF marker");

        self.find_backwards(b"trailer");
        self.chop_word();
        self.chop_while(Self::is_ascii_whitespace);

        if let Object::Dict(td) = self.chop_dict_obj() {
            self.trailer_dict = td;
        } else {
            unreachable!();
        }

        self.cur = xref_offset;
        self.fill_xref_table();

        for (k, v) in &self.xref_table {
            println!("{:#?}: {:#?},", k, v);
        }
        // Unfinished
        todo!();
    }

    fn fill_xref_table(&mut self) {
        if self.trailer_dict.is_empty() {
            panic!("Tried to parse xref table without trailer dictionary");
        }

        match self.chop_token() {
            Some(Token::Keyword(Keyword::Xref)) => {
                let start;
                if let Some(Token::Int(start_)) = self.chop_token() {
                    start = start_;
                } else {
                    panic!("index {}: expected integer after `xref`", self.cur);
                }

                let n_entries;
                if let Some(Token::Int(n_entries_)) = self.chop_token() {
                    n_entries = n_entries_;
                } else {
                    panic!("index {}: expected 2 integers after `xref`", self.cur);
                }

                for i in 0..n_entries {
                    let nref = (start + i) as usize;

                    let offset;
                    if let Some(Token::Int(offset_)) = self.chop_token() {
                        offset = offset_ as usize;
                    } else {
                        panic!("index {}: expected reference number", self.cur);
                    }

                    let ngen;
                    if let Some(Token::Int(ngen_)) = self.chop_token() {
                        ngen = ngen_ as u16;
                    } else {
                        panic!("index {}: expected generation number", self.cur);
                    }

                    match self.chop_token() {
                        Some(Token::Keyword(Keyword::EntryInUse)) => {
                            assert!(
                                ngen == 0,
                                "TODO: Add support for incrementally changed PDFs"
                            );
                            {
                                let saved = self.cur;
                                self.cur = offset;
                                let obj = self.chop_obj();
                                self.xref_table.insert(nref, obj);
                                self.cur = saved;
                            }
                        }

                        Some(Token::Keyword(Keyword::EntryFree)) => {
                            assert!(
                                ngen == 65535,
                                "TODO: Add support for incrementally changed PDFs"
                            );
                            // TODO: Keep track of free objects. They're completely ignored atm
                        }

                        _ => {
                            panic!("index {}: Expected either `n` or `f`", self.cur);
                        }
                    }
                }
            }

            _ => {
                panic!("index {}: expected keyword `xref`", self.cur);
            }
        }
    }

    fn chop_char(&mut self) -> Option<u8> {
        self.cur += 1;
        self.data.get(self.cur - 1).cloned().map(|ch| match ch {
            b'\r' => {
                if let Some(b'\n') = self.data.get(self.cur) {
                    self.cur += 1;
                }
                b'\n'
            }
            _ => ch,
        })
    }

    fn chop_char_backwards(&mut self) -> Option<u8> {
        self.cur -= 1;
        self.data.get(self.cur + 1).cloned().map(|ch| match ch {
            b'\r' => b'\n',
            b'\n' => {
                if let Some(b'\r') = self.data.get(self.cur) {
                    self.cur -= 1;
                }
                b'\n'
            }
            _ => ch,
        })
    }

    fn chop_n_chars(&mut self, n: usize) -> &'a [u8] {
        let begin = self.cur;
        for _ in 0..n {
            self.chop_char();
        }
        return &self.data[begin..self.cur];
    }

    fn slurp_n_bytes(&mut self, n: usize) -> &'a [u8] {
        let begin = self.cur;
        self.cur += n;
        return &self.data[begin..self.cur];
    }

    fn chop_while(&mut self, predicate: fn(u8) -> bool) -> &'a [u8] {
        let begin = self.cur;
        while predicate(self.data[self.cur]) {
            self.chop_char();
        }
        return &self.data[begin..self.cur];
    }

    fn chop_word(&mut self) -> &'a [u8] {
        self.chop_while(Self::is_ascii_normal)
    }

    fn chop_int<T: FromStr>(&mut self) -> Option<T> {
        let begin = self.cur;
        while self.data[self.cur].is_ascii_digit() {
            self.cur += 1;
        }
        T::from_str(
            str::from_utf8(&self.data[begin..self.cur]).expect("Tried to chop int from non-UTF8"),
        )
        .ok()
    }

    fn chop_token(&mut self) -> Option<Token> {
        self.chop_while(Self::is_ascii_whitespace);
        match self.data[self.cur] {
            b'<' => {
                self.chop_char();
                if let Some(b'<') = self.data.get(self.cur) {
                    self.chop_char();
                    return Some(Token::DictBegin);
                }
                // TODO: Add support for hexadecimal strings
                todo!("Hex string literals");
            }

            b'>' => {
                if let Some(b'>') = self.data.get(self.cur + 1) {
                    self.chop_n_chars(2);
                    return Some(Token::DictEnd);
                }
                unreachable!("chop_token() called on stray `>`");
            }

            b'[' => {
                self.chop_char();
                return Some(Token::ArrayBegin);
            }

            b']' => {
                self.chop_char();
                return Some(Token::ArrayEnd);
            }

            b'/' => {
                self.chop_char();
                return Some(Token::Solidus);
            }

            b'(' => {
                self.chop_char();
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
                                    while i < 3 && (b'0'..=b'7').contains(self.data.get(self.cur)?)
                                    {
                                        s.push(self.chop_char()? as char);
                                        i += 1;
                                    }
                                    // Already made sure everything is '0'..='7' in the loop
                                    result.push(u8::from_str_radix(&s, 8).unwrap());
                                }

                                _ => {
                                    panic!(
                                        "index {}: Invalid escape character `{}`",
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

            b'0'..=b'9' | b'.' | b'+' | b'-' => {
                let mut s = String::new();
                while matches!(self.data[self.cur], b'0'..=b'9' | b'.' | b'+' | b'-') {
                    s.push(self.chop_char()? as char);
                }
                let i = s.parse::<i64>();
                if i.is_err() {
                    let f = s
                        .parse::<f64>()
                        .expect(format!("index {}: Illegal float literal", self.cur).as_str());
                    return Some(Token::Float(f));
                }
                return Some(Token::Int(i.unwrap()));
            }

            _ => {
                // TODO: Add support for comments
                // TODO: Implement support for boolean objects
                // TODO: Add support for the null object
                let word = self.chop_word();

                str::from_utf8(word)
                    .expect("Tried to chop token from non-UTF-8 word")
                    .parse::<Keyword>()
                    .map(Token::Keyword)
                    .ok()
            }
        }
    }

    fn peek_token(&mut self) -> Option<Token> {
        let saved = self.cur;
        let result = self.chop_token();
        self.cur = saved;
        return result;
    }

    fn chop_array_obj(&mut self) -> Object<'a> {
        if self.chop_token() != Some(Token::ArrayBegin) {
            panic!("index {}: Expected an array", self.cur);
        }

        let mut result = Vec::new();

        loop {
            if self.peek_token() == Some(Token::ArrayEnd) {
                self.chop_token();
                return Object::Array(result);
            }

            let obj = self.chop_obj();
            result.push(obj);
        }
    }

    fn chop_dict_obj(&mut self) -> Object<'a> {
        if self.chop_token() != Some(Token::DictBegin) {
            panic!("index {}: Expected a dictionary", self.cur);
        }

        let mut result = HashMap::new();

        loop {
            if self.peek_token() == Some(Token::DictEnd) {
                self.chop_token();
                return Object::Dict(result);
            }
            let key;
            if let Object::Name(key_) = self.chop_obj() {
                key = key_;
            } else {
                panic!(
                    "index {}: Expected name object as key in dictionary",
                    self.cur
                );
            }

            let value = self.chop_obj();

            result.insert(key, value);
        }
    }

    fn chop_name_obj(&mut self) -> Object<'a> {
        if self.peek_token() != Some(Token::Solidus) {
            panic!("index {}: Expected a name object", self.cur);
        }
        self.chop_token();

        let name = str::from_utf8(self.chop_while(Self::is_ascii_normal))
            .expect("Name objects should be UTF-8");
        return Object::Name(name);
    }

    fn chop_stream_obj(&mut self, dict: HashMap<&'a str, Object<'a>>) -> Object<'a> {
        if self.chop_token() != Some(Token::Keyword(Keyword::Stream)) {
            panic!("index {}: Expected a stream", self.cur);
        }

        let length;
        let olength = dict
            .get("Length")
            .expect("Stream dictionary must have a `Length` field");
        match *olength {
            Object::Int(i) => {
                length = i as usize;
            }

            Object::RawReference(refnum, _gennum) => {
                if let Object::Int(i) = *self
                    .xref_table
                    .get(&(refnum as usize))
                    .expect("Illegal object reference")
                {
                    length = i as usize;
                } else {
                    panic!(
                        "index {}: `Length` in stream dictionary must be an integer.",
                        self.cur
                    );
                }
            }

            _ => {
                panic!(
                    "index {}: `Length` in stream dictionary must be an integer.",
                    self.cur
                );
            }
        }

        assert!(self.chop_char() == Some(b'\n'));
        let data = self.slurp_n_bytes(length);

        if self.chop_token() != Some(Token::Keyword(Keyword::EndStream)) {
            panic!("index {}: stream without endstream", self.cur);
        }

        // TODO: Decode data in stream objects
        return Object::Stream(dict, data);
    }

    fn chop_obj(&mut self) -> Object<'a> {
        match self.peek_token() {
            Some(Token::ArrayBegin) => self.chop_array_obj(),
            Some(Token::DictBegin) => self.chop_dict_obj(),
            Some(Token::Solidus) => self.chop_name_obj(),
            Some(Token::Int(i)) => {
                self.chop_token();
                let saved = self.cur;
                if let Some(Token::Int(gennum)) = self.peek_token() {
                    self.chop_token();
                    match self.peek_token() {
                        Some(Token::Keyword(Keyword::R)) => {
                            self.chop_token();
                            return Object::RawReference(i, gennum);
                        }

                        Some(Token::Keyword(Keyword::Obj)) => {
                            self.chop_token();
                            let ret = self.chop_obj();
                            match self.peek_token() {
                                Some(Token::Keyword(Keyword::EndObj)) => {
                                    self.chop_token();
                                    return ret;
                                }

                                Some(Token::Keyword(Keyword::Stream)) => {
                                    let dict;
                                    if let Object::Dict(dict_) = ret {
                                        dict = dict_;
                                    } else {
                                        panic!("index {}: Stream dictionary must be a dictionary object", self.cur);
                                    }
                                    let streamobj = self.chop_stream_obj(dict);
                                    if self.chop_token() != Some(Token::Keyword(Keyword::EndObj)) {
                                        panic!(
                                            "index {}: endobj must immediately follow endstream",
                                            self.cur
                                        );
                                    }
                                    return streamobj;
                                }

                                _ => {
                                    panic!("index {}: obj without endobj", self.cur);
                                }
                            }
                        }

                        _ => {
                            self.cur = saved;
                        }
                    }
                }
                Object::Int(i)
            }
            Some(Token::Float(f)) => {
                self.chop_token();
                Object::Float(f)
            }
            Some(Token::String(str)) => {
                self.chop_token();
                Object::String(str)
            }
            _ => {
                unimplemented!("{:?}", self.peek_token());
            }
        }
    }

    fn find_backwards(&mut self, target: &[u8]) {
        while !self.data[self.cur..].starts_with(target) {
            self.chop_char_backwards();
        }
    }

    fn is_ascii_normal(x: u8) -> bool {
        !Self::is_ascii_whitespace(x) && !Self::is_ascii_delim(x)
    }

    fn is_ascii_whitespace(x: u8) -> bool {
        matches!(x, b'\0' | b'\t' | b'\n' | b'\x0C' | b'\r' | b' ')
    }

    fn is_ascii_delim(x: u8) -> bool {
        matches!(
            x,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
    }
}

fn main() {
    let mut args = env::args();
    let _program = args.next();
    let path = args.next().unwrap_or("./test.pdf".to_owned());
    let data = fs::read(path).expect("Invalid file name provided");
    let _parser = Parser::new(data.as_slice());
}
