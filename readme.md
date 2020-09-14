# Reactive programming in Rust
An example of declaratively defining values, their derivations and the automatic propagation of changes and disposal of values

Supports:
- Wrap around a primitive 'value' with getters/setters
- Lazy evaluation of 'derived' values and automated propagation of changes (pull)
- 'Reactive' functions that are called when watched values are changed (push)
- Automated cleanup - thanks RAII and Reference counters :)

## Using
This code is just experimental and only meant just for looking at. 

Run the example via:
```
cargo +nightly run
```

## Todo
- Get the async version working (async closures are not pretty in Rust yet)
