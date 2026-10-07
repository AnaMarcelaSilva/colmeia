// Package rodar acha as configurações de execução de um projeto (o "Play"):
// as do IntelliJ (.idea/workspace.xml e .run/*.run.xml) e as que os arquivos
// do projeto sugerem (scripts do package.json, alvos do Makefile, Cargo,
// Go). Só lê; quem grava é a API, com o que você escolher.
package rodar

import (
	"encoding/json"
	"encoding/xml"
	"os"
	"path/filepath"
	"regexp"
	"runtime"
	"sort"
	"strings"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

// Arquivos maiores que isto não são lidos (um workspace.xml passa de 1 MB
// só em projetos muito grandes).
const maxArquivo = 8 << 20

// Sugestao é uma configuração achada, ainda não gravada.
type Sugestao struct {
	dados.CamposComando
	Origem string `json:"origem"`
	// De onde veio, para mostrar ("IntelliJ · Go", "package.json").
	Fonte string `json:"fonte"`
}

// Achados é o que a busca encontrou: as sugestões e, das configurações do
// IntelliJ, as de um tipo que a Colmeia ainda não sabe rodar.
type Achados struct {
	Sugestoes     []Sugestao `json:"sugestoes"`
	NaoSuportadas []string   `json:"nao_suportadas"`
}

// Procurar olha a pasta do projeto.
func Procurar(pasta string) Achados {
	a := Achados{Sugestoes: []Sugestao{}, NaoSuportadas: []string{}}
	intellij(pasta, &a)
	doProjeto(pasta, &a)
	// Um nome por sugestão: o IntelliJ vem primeiro e ganha.
	vistos := map[string]bool{}
	unicas := a.Sugestoes[:0]
	for _, s := range a.Sugestoes {
		if !vistos[s.Nome] {
			vistos[s.Nome] = true
			unicas = append(unicas, s)
		}
	}
	a.Sugestoes = unicas
	return a
}

func ler(caminho string) []byte {
	info, err := os.Stat(caminho)
	if err != nil || !info.Mode().IsRegular() || info.Size() > maxArquivo {
		return nil
	}
	b, _ := os.ReadFile(caminho)
	return b
}

// IntelliJ

type xmlOpcao struct {
	Nome  string `xml:"name,attr"`
	Valor string `xml:"value,attr"`
}

type xmlValor struct {
	Valor string `xml:"value,attr"`
}

type xmlConfiguracao struct {
	Nome       string     `xml:"name,attr"`
	Tipo       string     `xml:"type,attr"`
	Temporaria string     `xml:"temporary,attr"`
	Modelo     string     `xml:"default,attr"`
	Opcoes     []xmlOpcao `xml:"option"`
	Ambiente   []xmlOpcao `xml:"envs>env"`
	Pasta      xmlValor   `xml:"working_directory"`
	ParamsGo   xmlValor   `xml:"go_parameters"`
	Parametros xmlValor   `xml:"parameters"`
	Tipo2      xmlValor   `xml:"kind"`
	Pacote     xmlValor   `xml:"package"`
	Diretorio  xmlValor   `xml:"directory"`
	Arquivo    xmlValor   `xml:"filePath"`
	Saida      xmlValor   `xml:"output_directory"`
	Scripts    xmlValor   `xml:"scripts>script"`
	Pacote2    xmlValor   `xml:"package-json"`
	Comando2   xmlValor   `xml:"command"`
	Argumentos xmlValor   `xml:"arguments"`
}

func (c xmlConfiguracao) opcao(nome string) (string, bool) {
	for _, o := range c.Opcoes {
		if o.Nome == nome {
			return o.Valor, true
		}
	}
	return "", false
}

type xmlProjeto struct {
	Componentes []struct {
		Nome          string            `xml:"name,attr"`
		Configuracoes []xmlConfiguracao `xml:"configuration"`
	} `xml:"component"`
}

// Uma .run/*.run.xml tem <component name="ProjectRunConfigurationManager">
// com uma configuração dentro, igual ao workspace.xml.
func intellij(pasta string, a *Achados) {
	var configuracoes []xmlConfiguracao
	if b := ler(filepath.Join(pasta, ".idea", "workspace.xml")); b != nil {
		var p xmlProjeto
		if xml.Unmarshal(b, &p) == nil {
			for _, c := range p.Componentes {
				if c.Nome == "RunManager" {
					configuracoes = append(configuracoes, c.Configuracoes...)
				}
			}
		}
	}
	arquivos, _ := filepath.Glob(filepath.Join(pasta, ".run", "*.run.xml"))
	sort.Strings(arquivos)
	for _, f := range arquivos {
		var c struct {
			Configuracoes []xmlConfiguracao `xml:"configuration"`
		}
		if b := ler(f); b != nil && xml.Unmarshal(b, &c) == nil {
			configuracoes = append(configuracoes, c.Configuracoes...)
		}
	}
	for _, c := range configuracoes {
		// Sem nome é o modelo de um tipo; temporária é a que o IntelliJ cria
		// sozinho ao rodar um arquivo e some depois.
		if strings.TrimSpace(c.Nome) == "" || c.Modelo == "true" || c.Temporaria == "true" {
			continue
		}
		s, ok := daConfiguracao(pasta, c)
		if !ok {
			a.NaoSuportadas = append(a.NaoSuportadas, strings.TrimSpace(c.Nome))
			continue
		}
		a.Sugestoes = append(a.Sugestoes, s)
	}
}

// local troca $PROJECT_DIR$ pela pasta do projeto.
func local(pasta, v string) string {
	v = strings.ReplaceAll(v, "$PROJECT_DIR$", pasta)
	v = strings.ReplaceAll(v, "$USER_HOME$", os.Getenv("HOME"))
	return filepath.Clean(filepath.FromSlash(v))
}

// relativa devolve `caminho` relativo a `base`, ou ok falso se ficar fora.
func relativa(base, caminho string) (string, bool) {
	r, err := filepath.Rel(base, caminho)
	if err != nil || r == ".." || strings.HasPrefix(r, ".."+string(filepath.Separator)) {
		return "", false
	}
	if r == "." {
		return "", true
	}
	return filepath.ToSlash(r), true
}

var seguro = regexp.MustCompile(`^[A-Za-z0-9_./:=@%+,-]+$`)

// argumento põe aspas simples no que precisa delas no shell.
func argumento(v string) string {
	if v != "" && seguro.MatchString(v) {
		return v
	}
	if runtime.GOOS == "windows" {
		return "'" + strings.ReplaceAll(v, "'", "''") + "'"
	}
	return "'" + strings.ReplaceAll(v, "'", `'\''`) + "'"
}

func juntar(partes ...string) string {
	var r []string
	for _, p := range partes {
		if strings.TrimSpace(p) != "" {
			r = append(r, strings.TrimSpace(p))
		}
	}
	return strings.Join(r, " ")
}

func daConfiguracao(pasta string, c xmlConfiguracao) (Sugestao, bool) {
	s := Sugestao{Origem: "intellij"}
	s.Nome = strings.TrimSpace(c.Nome)
	for _, e := range c.Ambiente {
		s.Ambiente = append(s.Ambiente, dados.Variavel{Nome: e.Nome, Valor: e.Valor})
	}
	trabalho := pasta
	if c.Pasta.Valor != "" {
		trabalho = local(pasta, c.Pasta.Valor)
	}
	rel, ok := relativa(pasta, trabalho)
	if !ok {
		return s, false
	}
	s.Pasta = rel
	// Um caminho do projeto, relativo à pasta de trabalho.
	daqui := func(v string) string {
		p := local(pasta, v)
		if r, err := filepath.Rel(trabalho, p); err == nil {
			if r == "." {
				return "."
			}
			if !strings.HasPrefix(r, "..") {
				return "./" + filepath.ToSlash(r)
			}
			return filepath.ToSlash(r)
		}
		return p
	}
	switch c.Tipo {
	case "GoApplicationRunConfiguration":
		s.Fonte = "IntelliJ · Go"
		var alvo string
		switch c.Tipo2.Valor {
		case "FILE":
			alvo = daqui(c.Arquivo.Valor)
		case "DIRECTORY":
			alvo = daqui(c.Diretorio.Valor)
		default:
			alvo = c.Pacote.Valor
		}
		if alvo == "" {
			return s, false
		}
		if rodar, ok := c.opcao("run"); ok && rodar == "false" {
			// Só compila (o "Build" do IntelliJ), para a pasta de saída.
			saida := ""
			if c.Saida.Valor != "" {
				saida = "-o " + argumento(daqui(c.Saida.Valor)+"/")
			}
			s.Comando = juntar("go build", c.ParamsGo.Valor, saida, argumento(alvo))
		} else {
			s.Comando = juntar("go run", c.ParamsGo.Valor, argumento(alvo), c.Parametros.Valor)
		}
	case "GoTestRunConfiguration":
		s.Fonte = "IntelliJ · Go test"
		alvo := "./..."
		switch c.Tipo2.Valor {
		case "DIRECTORY":
			alvo = daqui(c.Diretorio.Valor) + "/..."
		case "PACKAGE":
			if c.Pacote.Valor != "" {
				alvo = c.Pacote.Valor
			}
		case "FILE":
			alvo = daqui(c.Arquivo.Valor)
		}
		s.Comando = juntar("go test", c.ParamsGo.Valor, argumento(alvo), c.Parametros.Valor)
	case "FlutterRunConfigurationType":
		s.Fonte = "IntelliJ · Flutter"
		arquivo, _ := c.opcao("filePath")
		args, _ := c.opcao("additionalArgs")
		if arquivo == "" {
			return s, false
		}
		s.Comando = juntar("flutter run -t", argumento(daqui(arquivo)), args)
	case "js.build_tools.npm":
		s.Fonte = "IntelliJ · npm"
		script := c.Scripts.Valor
		if script == "" {
			return s, false
		}
		if c.Pacote2.Valor != "" {
			if dir, ok := relativa(pasta, filepath.Dir(local(pasta, c.Pacote2.Valor))); ok {
				s.Pasta = dir
			}
		}
		comando := cmpOr(c.Comando2.Valor, "run")
		s.Comando = juntar("npm", comando, argumento(script), c.Argumentos.Valor)
	case "ShConfigurationType":
		s.Fonte = "IntelliJ · Shell"
		if dir, _ := c.opcao("SCRIPT_WORKING_DIRECTORY"); dir != "" {
			if r, ok := relativa(pasta, local(pasta, dir)); ok {
				s.Pasta = r
			}
		}
		if texto, _ := c.opcao("SCRIPT_TEXT"); strings.TrimSpace(texto) != "" {
			if usar, _ := c.opcao("EXECUTE_SCRIPT_FILE"); usar != "true" {
				s.Comando = texto
				break
			}
		}
		script, _ := c.opcao("SCRIPT_PATH")
		if script == "" {
			return s, false
		}
		opcoes, _ := c.opcao("SCRIPT_OPTIONS")
		interpretador, _ := c.opcao("INTERPRETER_PATH")
		s.Comando = juntar(interpretador, argumento(daqui(script)), opcoes)
	default:
		return s, false
	}
	return s, true
}

func cmpOr(a, b string) string {
	if a != "" {
		return a
	}
	return b
}

// Arquivos do projeto

func doProjeto(pasta string, a *Achados) {
	// package.json: um "npm run" por script.
	if b := ler(filepath.Join(pasta, "package.json")); b != nil {
		var p struct {
			Scripts map[string]string `json:"scripts"`
		}
		if json.Unmarshal(b, &p) == nil {
			nomes := make([]string, 0, len(p.Scripts))
			for n := range p.Scripts {
				nomes = append(nomes, n)
			}
			sort.Strings(nomes)
			gerenciador := "npm"
			switch {
			case existe(filepath.Join(pasta, "pnpm-lock.yaml")):
				gerenciador = "pnpm"
			case existe(filepath.Join(pasta, "yarn.lock")):
				gerenciador = "yarn"
			case existe(filepath.Join(pasta, "bun.lockb")), existe(filepath.Join(pasta, "bun.lock")):
				gerenciador = "bun"
			}
			for _, n := range nomes {
				a.Sugestoes = append(a.Sugestoes, sugestao(gerenciador+" "+n, gerenciador+" run "+argumento(n), "package.json"))
			}
		}
	}
	// Makefile: os alvos simples, sem os especiais (.PHONY) nem os de padrão (%).
	for _, nome := range []string{"Makefile", "makefile", "GNUmakefile"} {
		b := ler(filepath.Join(pasta, nome))
		if b == nil {
			continue
		}
		alvo := regexp.MustCompile(`(?m)^([A-Za-z0-9][A-Za-z0-9_.-]*)\s*:([^=]|$)`)
		vistos := map[string]bool{}
		for _, m := range alvo.FindAllStringSubmatch(string(b), -1) {
			if !vistos[m[1]] && len(vistos) < 30 {
				vistos[m[1]] = true
				a.Sugestoes = append(a.Sugestoes, sugestao("make "+m[1], "make "+m[1], nome))
			}
		}
		break
	}
	if existe(filepath.Join(pasta, "Cargo.toml")) {
		a.Sugestoes = append(a.Sugestoes, sugestao("cargo run", "cargo run", "Cargo.toml"), sugestao("cargo test", "cargo test", "Cargo.toml"))
	}
	if existe(filepath.Join(pasta, "go.mod")) {
		if existe(filepath.Join(pasta, "main.go")) {
			a.Sugestoes = append(a.Sugestoes, sugestao("go run .", "go run .", "go.mod"))
		}
		// Os programas em cmd/<nome>.
		dirs, _ := filepath.Glob(filepath.Join(pasta, "cmd", "*", "main.go"))
		sort.Strings(dirs)
		for _, d := range dirs {
			nome := filepath.Base(filepath.Dir(d))
			a.Sugestoes = append(a.Sugestoes, sugestao("go run "+nome, "go run ./cmd/"+argumento(nome), "go.mod"))
		}
		a.Sugestoes = append(a.Sugestoes, sugestao("go test", "go test ./...", "go.mod"))
	}
	if existe(filepath.Join(pasta, "pubspec.yaml")) {
		a.Sugestoes = append(a.Sugestoes, sugestao("flutter run", "flutter run", "pubspec.yaml"))
	}
}

func sugestao(nome, comando, fonte string) Sugestao {
	s := Sugestao{Origem: "projeto", Fonte: fonte}
	s.Nome, s.Comando = nome, comando
	return s
}

func existe(caminho string) bool {
	info, err := os.Stat(caminho)
	return err == nil && info.Mode().IsRegular()
}
