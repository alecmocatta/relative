# relative

[![Crates.io](https://img.shields.io/crates/v/relative.svg?maxAge=86400)](https://crates.io/crates/relative)
[![MIT / Apache 2.0 licensed](https://img.shields.io/crates/l/relative.svg?maxAge=2592000)](#License)
[![Build Status](https://dev.azure.com/alecmocatta/relative/_apis/build/status/tests?branchName=master)](https://dev.azure.com/alecmocatta/relative/_build/latest?branchName=master)

[Docs](https://docs.rs/relative/0.1.5)

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
Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE.txt](LICENSE-APACHE.txt) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT.txt](LICENSE-MIT.txt) or http://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
