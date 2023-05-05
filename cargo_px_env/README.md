# cargo_px_env

Utilities to retrieve the environment variables set by `cargo px`.

When `cargo px` invokes a code generator, it sets various environment variables that 
can be leveraged by the code generator to retrieve information about the workspace.  
This crate provides bindings to work with these environment variables instead 
of hard-coding their names in your code generator.