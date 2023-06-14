# Freed
Experimental **unfinished** PDF parser. For educational & exploratory purposes.
So far, only lexing the file is implemented. Potential for a weak PDF editor
later. 

The code itself is very likely atrocious - this is a beginner Rust project.

## Quick Start
The code expects a file called `test.pdf` in the project directory. 
```sh
cargo run
```

## References
 - ISO 32000-2:2020(E), taken from <https://pdfa.org>
 - Another parser in Rust I read to figure out how this language works:
   <https://github.com/tsoding/Noq>
