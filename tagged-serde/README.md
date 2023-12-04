## tagged-serde

This crate has a procedural macro for generating serde implementations for enums (a.k.a. tagged unions)
with integer tags. In the absence of thorough documentation, the main idea is that

```rust
#[derive(TaggedSerde)]
enum MyEnum {
  #[tagged_serde = 1]
  Str(String),
  #[tagged_serde = 42]
  Int(i32),
}
```

will define `serde::Serialize` and `serde::Deserialize` implementations for `MyEnum` so that
`MyEnum::Str("hi")` will get serialized as `(1, "hi")` and `MyEnum::Int(5)` will get
serialized as `(42, 5)`.
