package main

import "C"
import (
	"cdr.dev/slog"
	"cdr.dev/slog/sloggers/sloghuman"
	"context"
	"encoding/json"
	"errors"
	"io"
	"oss.terrastruct.com/d2/d2compiler"
	"oss.terrastruct.com/d2/d2graph"
	"oss.terrastruct.com/d2/d2layouts/d2dagrelayout"
	"oss.terrastruct.com/d2/d2lib"
	"oss.terrastruct.com/d2/d2parser"
	"oss.terrastruct.com/d2/d2renderers/d2svg"
	"oss.terrastruct.com/d2/d2target"
	"oss.terrastruct.com/d2/lib/imgbundler"
	"oss.terrastruct.com/d2/lib/log"
	"oss.terrastruct.com/d2/lib/simplelog"
	"oss.terrastruct.com/d2/lib/textmeasure"
	"strings"
)

const ErrorPrefix = "err:"

type RenderResult struct {
	Name         string `json:"name"`
	IsFolderOnly bool   `json:"isFolderOnly"`
	Content      string `json:"content"`

	Root *d2graph.Object `json:"root"`

	Layers    []RenderResult `json:"layers"`
	Scenarios []RenderResult `json:"scenarios"`
	Steps     []RenderResult `json:"steps"`
}

//export Render
func Render(content string) *C.char {
	return serializeResult(render(content))
}

func render(content string) (*RenderResult, error) {
	ctx := log.With(context.Background(), slog.Make(sloghuman.Sink(io.Discard)))
	ruler, err := textmeasure.NewRuler()
	if err != nil {
		return nil, err
	}
	layoutResolver := func(engine string) (d2graph.LayoutGraph, error) {
		return d2dagrelayout.DefaultLayout, nil
	}
	renderOpts := &d2svg.RenderOpts{}

	diagram, graph, err := d2lib.Compile(ctx, content,
		&d2lib.CompileOptions{
			LayoutResolver: layoutResolver,
			Ruler:          ruler,
		},
		renderOpts,
	)
	if err != nil {
		return nil, err
	}

	return renderRecursive(diagram, graph, renderOpts, ctx)
}

func renderRecursive(
	diagram *d2target.Diagram,
	graph *d2graph.Graph,
	renderOpts *d2svg.RenderOpts,
	ctx context.Context,
) (*RenderResult, error) {
	var layers []RenderResult
	var scenarios []RenderResult
	var steps []RenderResult

	if len(diagram.Layers) != len(graph.Layers) {
		return nil, errors.New("layers count mismatch")
	}
	for i, layer := range diagram.Layers {
		layerGraph := graph.Layers[i]
		layerResult, err := renderRecursive(layer, layerGraph, renderOpts, ctx)
		if err != nil {
			return nil, err
		}
		layers = append(layers, *layerResult)
	}

	if len(diagram.Scenarios) != len(graph.Scenarios) {
		return nil, errors.New("scenarios count mismatch")
	}
	for i, scenario := range diagram.Scenarios {
		scenarioGraph := graph.Scenarios[i]
		scenarioResult, err := renderRecursive(scenario, scenarioGraph, renderOpts, ctx)
		if err != nil {
			return nil, err
		}
		scenarios = append(scenarios, *scenarioResult)
	}

	if len(diagram.Steps) != len(graph.Steps) {
		return nil, errors.New("steps count mismatch")
	}
	for i, step := range diagram.Steps {
		stepGraph := graph.Steps[i]
		stepResult, err := renderRecursive(step, stepGraph, renderOpts, ctx)
		if err != nil {
			return nil, err
		}
		steps = append(steps, *stepResult)
	}

	svg, err := d2svg.Render(diagram, renderOpts)
	if err != nil {
		return nil, err
	}

	// we don't have a filesystem setup to pull images from
	svg, err = imgbundler.BundleRemote(ctx, simplelog.FromLibLog(ctx), svg, false)
	if err != nil {
		return nil, err
	}

	return &RenderResult{
		Name:         diagram.Name,
		IsFolderOnly: diagram.IsFolderOnly,
		Content:      string(svg),
		Root:         graph.Root,
		Layers:       layers,
		Scenarios:    scenarios,
		Steps:        steps,
	}, nil
}

//export Compile
func Compile(content string) *C.char {
	graph, _, err := d2compiler.Compile("", strings.NewReader(content), nil)

	return serializeResult(graph, err)
}

//export Parse
func Parse(content string) *C.char {
	ast, err := d2parser.Parse("", strings.NewReader(content), nil)

	return serializeResult(ast, err)
}

type D2Error struct {
	Message    string               `json:"message"`
	ParseError *d2parser.ParseError `json:"parse_error"`
}

func serializeResult(value any, err error) *C.char {
	if err != nil {
		return serializeError(err)
	}

	result, err := json.Marshal(value)
	//result, err := json.MarshalIndent(value, "", "  ")

	if err != nil {
		return serializeError(err)
	}

	return C.CString(string(result))
}

func serializeError(err error) *C.char {
	resultErr := D2Error{}

	var parseErr *d2parser.ParseError
	if errors.As(err, &parseErr) {
		resultErr.ParseError = parseErr
	} else {
		resultErr.Message = err.Error()
	}

	result, err := json.Marshal(resultErr)
	if err != nil {
		return C.CString(ErrorPrefix + err.Error())
	}

	return C.CString(ErrorPrefix + string(result))
}

func main() {}
