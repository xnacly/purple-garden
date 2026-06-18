```text
         ,            ,            ,
     /\^/`\       /\^/`\       /\^/`\
    | \/   |     | \/   |     | \/   |
    | |    |     | |    |     | |    |
    \ \    /     \ \    /     \ \    /
     '\\//'       '\\//'       '\\//'
       ||           ||           ||
       ||           ||           ||
       ||           ||           ||
       ||  ,        ||  ,        ||  ,
   |\  ||  |\   |\  ||  |\   |\  ||  |\
   | | ||  | |  | | ||  | |  | | ||  | |
   | | || / /   | | || / /   | | || / /
    \ \||/ /     \ \||/ /     \ \||/ /
     `\\//`       `\\//`       `\\//`
    ^^^^^^^^     ^^^^^^^^     ^^^^^^^^
```

# Purple Garden

Purple Garden is a high-performance interpreted programming language with a JIT and a minimalist standard library. It is built to be embedded and extended from Rust.

It can be used in performance-sensitive contexts and allows disabling common sources of runtime cost, including `--no-std`, `--no-env`, and `--no-gc`.

## Topics

Use:

```sh
purple-garden intro <topic>
```

Available topics include:

- `types`: primitive and non-primitive types, operators, and casts.
- `embed`: embedding Purple Garden in a Rust project.

## Command Line

```sh
purple-garden --help
```

## Standard Library

Use `doc` to inspect packages, methods and language keywords:

```sh
purple-garden doc <pkg>[.<method>]
purple-garden doc <keyword>
```

For example:

```sh
purple-garden doc io.println
```
