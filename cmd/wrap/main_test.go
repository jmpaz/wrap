package main

import "testing"

func TestUnwrapMdWithLabel(t *testing.T) {
	input := "```foo\nbar\n```"
	out := unwrapMd(input)
	if out != input {
		t.Errorf("expected input unchanged, got %q", out)
	}
}

func TestUnwrapMdSimple(t *testing.T) {
	input := "```\nbar\n```"
	out := unwrapMd(input)
	if out != "bar" {
		t.Errorf("expected unwrapped content, got %q", out)
	}
}

func TestIsAlreadyWrappedLabel(t *testing.T) {
	input := "```foo\nbar\n```"
	if isAlreadyWrapped(input, "md") {
		t.Errorf("expected not detected as wrapped")
	}
}

func TestUnwrapXmlCodeFence(t *testing.T) {
	input := "<paste>\n```foo\nbar\n```\n</paste>"
	expect := "```foo\nbar\n```"
	out := unwrapXml(input)
	if out != expect {
		t.Errorf("expected %q, got %q", expect, out)
	}
}
