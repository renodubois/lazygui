// M0 standard-library oracle, not a production helper or application dependency.
// Only fixture commands exist. It never invokes a shell, Git, accounts, or providers.
package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strings"
	"text/template"
)

type Case struct {
	Name     string
	Template string
	Data     map[string]any
	Commands map[string]string
	Trusted  bool
	Expected string
	Error    bool
}

func evaluate(c Case) (string, error) {
	// Approval precedes evaluation, not merely the final shell command.
	if !c.Trusted {
		return "", fmt.Errorf("source requires approval")
	}
	functions := template.FuncMap{
		"quote": func(value string) string { return "'" + strings.ReplaceAll(value, "'", "'\\''") + "'" },
		"runCommand": func(command string) (string, error) {
			value, ok := c.Commands[command]
			if !ok {
				return "", fmt.Errorf("only fixture commands allowed")
			}
			return value, nil
		},
	}
	t, err := template.New("fixture").Funcs(functions).Parse(c.Template)
	if err != nil {
		return "", err
	}
	var output bytes.Buffer
	err = t.Execute(&output, c.Data)
	return output.String(), err
}
func main() {
	var cases []Case
	if err := json.NewDecoder(io.LimitReader(os.Stdin, 1024*1024)).Decode(&cases); err != nil {
		panic(err)
	}
	for _, c := range cases {
		result, err := evaluate(c)
		if (err != nil) != c.Error || (!c.Error && result != c.Expected) {
			fmt.Fprintf(os.Stderr, "FAIL %s: expected result/error mismatch\n", c.Name)
			os.Exit(1)
		}
		fmt.Println("PASS", c.Name)
	}
}
