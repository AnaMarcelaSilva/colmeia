// Package sessoes lê as conversas que o Claude Code guardou numa pasta, para a
// tarefa retomar uma conversa começada em outro lugar (no IntelliJ, num
// terminal). Só lê o fim de cada arquivo: as conversas passam de dezenas de MB.
package sessoes

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"
	"unicode/utf8"
)

// Quanto do fim de cada conversa é lido para achar o título.
const tamanhoFim = 256 * 1024

// Maior quantidade de conversas listadas por pasta.
const maximo = 30

type Sessao struct {
	ID       string    `json:"id"`
	Titulo   string    `json:"titulo"`
	Alterada time.Time `json:"alterada"`
	Tamanho  int64     `json:"tamanho"`
}

var padraoID = regexp.MustCompile(`^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`)

// IDValido confere que o id tem a forma dos ids do Claude Code, antes de ir
// para a linha de comando.
func IDValido(id string) bool { return padraoID.MatchString(id) }

var naoAlfanumerico = regexp.MustCompile(`[^A-Za-z0-9]`)

// PastaDoProjeto é onde o Claude Code guarda as conversas de uma pasta de
// trabalho: o caminho com tudo que não é letra ou número trocado por '-'.
func PastaDoProjeto(configuracao, dir string) string {
	return filepath.Join(configuracao, "projects", naoAlfanumerico.ReplaceAllString(dir, "-"))
}

// PastaDeConfiguracao é a pasta de configuração do Claude Code: a conta
// separada do perfil, se houver, ou a do sistema.
func PastaDeConfiguracao(separada string) (string, error) {
	if separada != "" {
		return separada, nil
	}
	if d := os.Getenv("CLAUDE_CONFIG_DIR"); d != "" {
		return d, nil
	}
	casa, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(casa, ".claude"), nil
}

// Listar devolve as conversas da pasta `dir`, da mais recente para a mais antiga.
func Listar(configuracao, dir string) ([]Sessao, error) {
	entradas, err := os.ReadDir(PastaDoProjeto(configuracao, dir))
	if os.IsNotExist(err) {
		return []Sessao{}, nil
	}
	if err != nil {
		return nil, err
	}
	lista := []Sessao{}
	for _, e := range entradas {
		id, ok := strings.CutSuffix(e.Name(), ".jsonl")
		if !ok || !IDValido(id) || !e.Type().IsRegular() {
			continue
		}
		info, err := e.Info()
		if err != nil || info.Size() == 0 {
			continue
		}
		lista = append(lista, Sessao{ID: id, Alterada: info.ModTime(), Tamanho: info.Size()})
	}
	sort.Slice(lista, func(i, j int) bool { return lista[i].Alterada.After(lista[j].Alterada) })
	lista = lista[:min(len(lista), maximo)]
	for i := range lista {
		lista[i].Titulo = titulo(filepath.Join(PastaDoProjeto(configuracao, dir), lista[i].ID+".jsonl"))
	}
	return lista, nil
}

// titulo procura, no fim da conversa, o nome dado por você, o título que o
// Claude Code gerou ou a última mensagem enviada, nessa ordem.
func titulo(caminho string) string {
	arquivo, err := os.Open(caminho)
	if err != nil {
		return ""
	}
	defer arquivo.Close()
	info, err := arquivo.Stat()
	if err != nil {
		return ""
	}
	inicio := max(0, info.Size()-tamanhoFim)
	fim := make([]byte, info.Size()-inicio)
	if _, err := arquivo.ReadAt(fim, inicio); err != nil {
		return ""
	}
	var escolhido, gerado, ultima string
	for _, linha := range bytes.Split(fim, []byte{'\n'}) {
		// Só as linhas pequenas de metadados interessam; as mensagens ficam de fora.
		if len(linha) > 4096 || !bytes.Contains(linha, []byte(`"type":"`)) {
			continue
		}
		var m struct {
			Type        string `json:"type"`
			CustomTitle string `json:"customTitle"`
			AITitle     string `json:"aiTitle"`
			LastPrompt  string `json:"lastPrompt"`
		}
		if json.Unmarshal(linha, &m) != nil {
			continue
		}
		switch m.Type {
		case "custom-title":
			escolhido = m.CustomTitle
		case "ai-title":
			gerado = m.AITitle
		case "last-prompt":
			ultima = m.LastPrompt
		}
	}
	for _, t := range []string{escolhido, gerado, ultima} {
		if t = limpar(t); t != "" {
			return t
		}
	}
	return ""
}

// limpar deixa o texto numa linha só e curto, sem caracteres de controle.
func limpar(t string) string {
	t = strings.Join(strings.Fields(strings.Map(func(r rune) rune {
		if r < ' ' || r == 0x7f {
			return ' '
		}
		return r
	}, t)), " ")
	if utf8.RuneCountInString(t) > 120 {
		t = string([]rune(t)[:119]) + "…"
	}
	return t
}
