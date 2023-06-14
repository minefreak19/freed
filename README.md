# Freed
Experimental **unfinished** PDF parser. For educational & exploratory purposes.
So far, only lexing the file is implemented. Potential for a weak PDF editor
later. 

The code itself is very likely atrocious - this is a beginner Rust project.

## Quick Start
The code runs on the pdf file specified in the first command line argument. If
not specified, it defaults to "./test.pdf".

```sh
cargo run ./path/to/pdf-file.pdf
cargo run # opens ./test.pdf
```

## References
 - ISO 32000-2:2020(E), taken from <https://pdfa.org>
 - Another parser in Rust I read to figure out how this language works:
   <https://github.com/tsoding/Noq>
