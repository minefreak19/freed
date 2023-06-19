# Freed
Experimental **unfinished** PDF parser. For educational & exploratory purposes.

The code itself is very likely atrocious - this is a beginner Rust project.

## Quick Start
The code runs on the pdf file specified in the first command line argument. If
not specified, it defaults to "./test.pdf".

```sh
cargo run -- ./path/to/pdf-file.pdf
cargo run # opens ./test.pdf
```

## Background
Initially, I started out trying to write this with a lexer and a parser
separately. I found this abstraction to be hindering progress rather than
aiding it, as at many points I found you needed to poke holes through the lexer
and access individual characters directly (e.g. in the case of streams,
indirect object references). Further, this approach relied too much on reading
the file from start to end, which is not the recommended way. (The
specification recommends parsers read the file from back to front, as there is
a table of all objects in the PDF located at the end of the file).

Consequentially, I had to rewrite it to have simply one `Parser` struct that
performs the functions of both a Lexer and a Parser - it can deal with
individual bytes directly, but also classify them into tokens if necessary.
*Very* little is implemented thus far, but this model seems to show much more
potential than the previous one. This is just an exploratory project for me to
understand the PDF file format better, but perhaps this might turn into a
full-fledged PDF parser/reader later.

## References
 - ISO 32000-2:2020(E), taken from <https://pdfa.org>
 - Another parser in Rust I read to figure out how this language works:
   <https://github.com/tsoding/Noq>
 - Lexer: https://en.wikipedia.org/wiki/Lexical_analysis
