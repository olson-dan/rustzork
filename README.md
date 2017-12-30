# rustzork
ZMachine V3 in Rust, now with WebAssembly support.

It is not entirely finished.  There is no save/restore, but otherwise Zork 1 is playable.  Unfortunately, there have been small bugs noticed in Zork 1 playthroughs despite the fact that the implementation passes the V3 CZECH tests (https://github.com/DustinCampbell/ZGo/tree/master/zcode/czech).

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