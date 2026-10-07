package rodar

import (
	"os"
	"path/filepath"
	"testing"
)

const workspace = `<?xml version="1.0" encoding="UTF-8"?>
<project version="4">
  <component name="RunManager" selected="Go Build.api">
    <configuration name="api" type="GoApplicationRunConfiguration" factoryName="Go Application">
      <working_directory value="$PROJECT_DIR$/loja-api" />
      <parameters value="-dev" />
      <envs><env name="PORTA" value="8081" /></envs>
      <kind value="PACKAGE" />
      <package value="loja-api/web" />
      <directory value="$PROJECT_DIR$" />
    </configuration>
    <configuration name="api" type="GoApplicationRunConfiguration" factoryName="Go Application" temporary="true">
      <kind value="FILE" />
      <filePath value="$PROJECT_DIR$/outro.go" />
    </configuration>
    <configuration name="compilar" type="GoApplicationRunConfiguration" factoryName="Go Application">
      <working_directory value="$PROJECT_DIR$/loja-api" />
      <go_parameters value="-trimpath" />
      <kind value="PACKAGE" />
      <package value="loja-api/cmd" />
      <output_directory value="$PROJECT_DIR$/loja-api/bin" />
      <option name="run" value="false" />
    </configuration>
    <configuration name="fora" type="GoApplicationRunConfiguration" factoryName="Go Application">
      <working_directory value="/tmp" />
      <kind value="PACKAGE" />
      <package value="x" />
    </configuration>
    <configuration name="servidor" type="QuarkusRunConfigurationType" factoryName="Quarkus" />
    <configuration name="script" type="ShConfigurationType">
      <option name="SCRIPT_TEXT" value="echo oi" />
      <option name="EXECUTE_SCRIPT_FILE" value="false" />
      <option name="SCRIPT_WORKING_DIRECTORY" value="$PROJECT_DIR$/loja-api" />
    </configuration>
    <configuration default="true" type="GoTestRunConfiguration" factoryName="Go Test" />
  </component>
</project>`

const arquivoRun = `<component name="ProjectRunConfigurationManager">
  <configuration default="false" name="testes" type="GoTestRunConfiguration" factoryName="Go Test">
    <kind value="DIRECTORY" />
    <directory value="$PROJECT_DIR$/loja-api" />
  </configuration>
</component>`

func escrever(t *testing.T, caminho, texto string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(caminho), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(caminho, []byte(texto), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestProcurarNoIntelliJENoProjeto(t *testing.T) {
	p := t.TempDir()
	escrever(t, filepath.Join(p, ".idea", "workspace.xml"), workspace)
	escrever(t, filepath.Join(p, ".run", "testes.run.xml"), arquivoRun)
	escrever(t, filepath.Join(p, "package.json"), `{"scripts":{"dev":"vite","build:web":"vite build"}}`)
	escrever(t, filepath.Join(p, "pnpm-lock.yaml"), "")
	escrever(t, filepath.Join(p, "Makefile"), ".PHONY: teste\nteste:\n\tgo test\n%.o: %.c\nVAR := 1\nbuild: teste\n")
	escrever(t, filepath.Join(p, "go.mod"), "module loja\n")
	escrever(t, filepath.Join(p, "cmd", "loja", "main.go"), "package main\n")

	a := Procurar(p)
	quer := map[string][2]string{
		"api":            {"go run loja-api/web -dev", "loja-api"},
		"compilar":       {"go build -trimpath -o ./bin/ loja-api/cmd", "loja-api"},
		"testes":         {"go test ./loja-api/...", ""},
		"pnpm build:web": {"pnpm run build:web", ""},
		"pnpm dev":       {"pnpm run dev", ""},
		"make teste":     {"make teste", ""},
		"make build":     {"make build", ""},
		"go run loja":    {"go run ./cmd/loja", ""},
		"go test":        {"go test ./...", ""},
		"script":         {"echo oi", "loja-api"},
	}
	achou := map[string]Sugestao{}
	for _, s := range a.Sugestoes {
		achou[s.Nome] = s
	}
	for nome, q := range quer {
		s, ok := achou[nome]
		if !ok || s.Comando != q[0] || s.Pasta != q[1] {
			t.Errorf("%s: %+v, quer %v", nome, s, q)
		}
	}
	if len(achou) != len(quer) {
		t.Errorf("sugestões a mais ou a menos: %+v", a.Sugestoes)
	}
	if v := achou["api"].Ambiente; len(v) != 1 || v[0].Nome != "PORTA" || v[0].Valor != "8081" || achou["api"].Origem != "intellij" {
		t.Errorf("api: %+v", achou["api"])
	}
	if len(a.NaoSuportadas) != 2 || a.NaoSuportadas[0] != "fora" || a.NaoSuportadas[1] != "servidor" {
		t.Errorf("não suportadas: %v", a.NaoSuportadas)
	}
}

func TestProcurarNumaPastaVazia(t *testing.T) {
	a := Procurar(t.TempDir())
	if len(a.Sugestoes) != 0 || len(a.NaoSuportadas) != 0 {
		t.Errorf("%+v", a)
	}
}
