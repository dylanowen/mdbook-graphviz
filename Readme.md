# mdBook Graphviz

## Install

```
cargo instal mdbook-graphviz
```

Install [Graphvis](https://graphviz.gitlab.io/download/)
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
processed -> graph
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
processed -> graph
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
skip -> graph
```
~~~

#### Output
~~~markdown
```dot
skip -> graph
```
~~~

## .gitignore

The generated svg files are output into the book src folder for now, this `.gitignore` should cover them

```
*.generated.svg
```
