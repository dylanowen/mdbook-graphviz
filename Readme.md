# mdBook Graphviz

### Install

Assumes that `dot` is installed

```toml
[preprocessor.graphviz]
command = "mdbook-graphviz"
```

## Usage

#### Ignored `dot`
~~~markdown
```dot
skip -> graph
```
~~~

~~~markdown
```dot
skip -> graph
```
~~~

#### Processed `dot`
~~~markdown
```dot process
processed -> graph
```
~~~

~~~markdown
![](chapter_0.generated.svg)
~~~

#### Processed `dot` With Name
~~~markdown
```dot process Named Graph
processed -> graph
```
~~~

~~~markdown
![](chapter_named_graph_0.generated.svg, "Named Graph")
~~~

## .gitignore
```
*.generated.svg
```
