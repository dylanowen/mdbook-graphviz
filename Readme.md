# mdBook Graphviz

[![crates.io](https://img.shields.io/crates/v/mdbook-graphviz.svg)](https://crates.io/crates/mdbook-graphviz)
[![LICENSE](https://img.shields.io/github/license/dylanowen/mdbook-graphviz.svg)](LICENSE)

## Install

```
cargo install mdbook-graphviz
```

Install [Graphviz](https://graphviz.gitlab.io/download/)
```
brew install graphviz
```

`book.toml`
```toml
[preprocessor.graphviz]
command = "mdbook-graphviz"
```

## Usage

Just `dot` is supported, but any of the other graphviz tools would be easy to add.

### Mark A `dot` Code Block For Processing

#### Input
~~~markdown
```dot process
digraph {
    "processed" -> "graph"
}
```
~~~

#### Output
~~~markdown
![](chapter_0.generated.svg)
~~~

#### Rendered
![](sample_0.generated.svg)

### Add A Name For Your Graph

#### Input
~~~markdown
```dot process Named Graph
digraph {
    "processed" -> "graph"
}
```
~~~

#### Output
~~~markdown
![](chapter_named_graph_0.generated.svg, "Named Graph")
~~~

#### Rendered
![](sample_0.generated.svg "Named Graph")

### `dot` Code Blocks Without The `process` Flag Are Ignored

#### Input
~~~markdown
```dot
digraph {
    "processed" -> "graph"
}
```
~~~

#### Output
~~~markdown
```dot
digraph {
    "processed" -> "graph"
}
```
~~~

## .gitignore

The generated svg files are output into the book src folder for now, this `.gitignore` should cover them

```
*.generated.svg
```
