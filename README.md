# rustzork
ZMachine V3 in Rust, now with WebAssembly support.

There is no save/restore, but otherwise Zork 1 is playable, and the implementation passes the V3 CZECH tests (https://github.com/DustinCampbell/ZGo/tree/master/zcode/czech).

I've only known Rust for like ~~2 weeks~~ a few months so this might suck.

Partially ported from my F# one.

Read my shitty blog post about zmachines: http://grumpygeneralist.blogspot.com/2014/08/write-zmachine.html

## WASM instructions

This guy's blog post provided me with a lot of "inspiration":
https://aimlesslygoingforward.com/blog/2017/12/25/dose-response-ported-to-webassembly/

To do things:

* Clone the project.
* Obtain a copy of `zork.z3` and put it into the root dir.
* `> rustup update nightly`
* `> rustup target add wasm32-unknown-unknown --toolchain=nightly`
* `> cargo build --release --target wasm32-unknown-unknown --no-default-features`
* `> python -m SimpleHTTPServer`
* Browse to http://localhost:8000

## Comments on this project

### CZECH zmachine checker

CZECH is not really that useful if your goal is to play Zork 1.  In three separate zmachine impelmentations I have had the same bug, namely that `insert_obj` must read the object pointed to by its second argument *after* the object pointed to by its first argument has been removed from the tree.  I tended to read both arguments up front and then do all of the transformations.  However the object tree is modified in the process of removing the first object, so the usage of the second object needs to reflect this.  This issue is not caught by CZECH.

CZECH *does* catch a number of issues.  The indirectable opcodes (`inc`, `dec`, and several others, noted as taking "`(variable)`" arguments in the spec and using `read_args!(Variable)` in my implementation) have very interesting and non-obvious behaviors regarding stack manipulation.  For instance `load` and `store` never push or pop the stack; they only manipulate it in place.  While Zork uses both `load` and `store`, I am not sure whether it depends on this behavior because I have not seen much difference either way.

Additionally the indirectable opcodes can take a variable argument in V3, which is noted as a tiny footnote in the spec that's easy to overlook.  CZECH will verify that this is done correctly, but I do not believe this functionality is exercised by Zork 1.

Finally CZECH did catch several issues with signed overflow.  Although the spec indicates that it's undefined, intepreters are encouraged to handle it with a mod by 0x10000.  This is also what CZECH requires, and I have not seen any evidence to make me believe that Zork 1 requires this behavior to be implemented.

I don't believe CZECH is maintained, but it would be good to extend it with test cases for the `insert_obj` bug mentioned above and more thorough checking of string decoding as there are subtle ways this can go wrong.  I have wanted for a while to have a test ROM that, if passing correctly, would guarantee that Zork 1 works correctly but putting it together would be very time consuming.

### Rust and WebAssembly

If you read the blog post linked above you probably know that zmachine is my go to project for learning a new language, and indeed that is why I attempted it in Rust.  I did not know much Rust when I started, and currently I think I only have about a 30% understanding of the language.  There are some problems, but for the most part I like it very much.

The WebAssembly version kind of sucks.  I mostly did it because it was easy, but I ran into several problems.  Calling between JS and WebAssembly is easy, which is great!  However passing data between them is pretty bad.  I'm sure that there are crates to make it easier, but even with that issue it was not very difficult to get it up and running.

I did run into what is most likely a compiler bug in the Rust `wasm32-unknown-unknown` target.   I rewrote the code to get around it and neglected to report the bug because I was not interested in making a reproducible test case.  I realize this is bad of me, but I hope that the target improves in the future and this won't be an issue anymore.


### Finishing the project

I don't really have any interest in implementing save, load, and restart.  I may become interested in improving the JS-side interface for the WASM target at some point, but not likely anytime soon.

I'm more interested in doing something like a MDL compiler or an assembler, but who knows if I ever will.
