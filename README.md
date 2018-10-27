# relative

[![Crates.io](https://img.shields.io/crates/v/relative.svg?style=flat-square&maxAge=86400)](https://crates.io/crates/relative)
[![Apache-2.0 licensed](https://img.shields.io/crates/l/relative.svg?style=flat-square&maxAge=2592000)](LICENSE.txt)
[![Build Status](https://ci.appveyor.com/api/projects/status/github/alecmocatta/relative?branch=master&svg=true)](https://ci.appveyor.com/project/alecmocatta/relative)
[![Build Status](https://circleci.com/gh/alecmocatta/relative/tree/master.svg?style=shield)](https://circleci.com/gh/alecmocatta/relative)
[![Build Status](https://travis-ci.com/alecmocatta/relative.svg?branch=master)](https://travis-ci.com/alecmocatta/relative)

[Docs](https://docs.rs/relative/0.1.3)

A type to wrap `&'static` references such that they can be safely sent between
other processes running the same binary.

References are adjusted relative to a base when (de)serialised, which is what
enables it to work across binaries that are dynamically loaded at different
addresses under multiple invocations.

It being the same binary is checked by serialising the
[`build_id`](https://docs.rs/build_id) alongside the relative pointer, which is
validated at deserialisation.

## Example
### Local process
```rust
let x: &'static [u16;4] = &[2,3,5,8];
// unsafe as it's up to the user to ensure the reference is into static memory
let relative = unsafe{Data::from(x)};
// send `relative` to remote...
```
### Remote process
```rust
// receive `relative`
println!("{:?}", relative.to());
// prints "[2, 3, 5, 8]"
```

## Note

This currently requires Rust nightly.

## License
Licensed under Apache License, Version 2.0, ([LICENSE.txt](LICENSE.txt) or
http://www.apache.org/licenses/LICENSE-2.0).

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
licensed as above, without any additional terms or conditions.
