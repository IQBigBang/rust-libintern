## Libintern

A simple but correct interning library.

```rust
let mut interner = Interner::new();

let a = interner.intern('a');
let other_a = interner.intern('a');

assert_eq!(a, other_a);
```