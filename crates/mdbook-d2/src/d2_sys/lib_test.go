package main

import "testing"

func TestRender(t *testing.T) {
	result, err := render(`
vars: {
  d2-config: {
    theme-id: 300
  }
}

Chicken's plan: {
  style.font-size: 35
  near: top-center
  shape: text
}

steps: {
  1: {
    Approach road
  }
  2: {
    Approach road -> Cross road
  }
  3: {
    Cross road -> Make you wonder why
  }
}`)
	if err != nil {
		t.Errorf("Render failed: %v", err)
	}

	println(result)
}
