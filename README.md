# MapExplorer

Simple map rendering application to develop Mapnik maps.

https://github.com/user-attachments/assets/efdea061-6cb9-46ca-98df-c61b6988702b

## Usage

```sh
map-explorer [path/to/map.xml] [base/path]
```

When map.xml is changed, the map will be automatically reloaded.

## Building

This project requires Rust and a C++ compiler.

```sh
cargo build
```

## Supported platforms

| Platform | Status |
|----------|--------|
| macOS    | :white_check_mark: |
| Linux    | :white_circle: |
| Windows  | :white_circle: |
| Web      | :gear: |

- :white_check_mark: = tested and working
- :white_circle: = untested, but presumable working
- :gear: = unimplemented, but planned

## Features

- Hot reloading of map.xml
- Panning, zooming
- Changing projections of input coordinates and map output
