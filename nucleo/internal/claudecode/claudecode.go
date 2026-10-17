// Package claudecode faz um pedido só ao Claude Code, sem terminal: o texto
// vai pela entrada padrão, a resposta volta no envelope do
// `--output-format json`. Sem nenhuma ferramenta, sem servidores MCP, sem
// configurações de projeto ou de usuário e sem guardar a conversa, numa pasta
// vazia apagada depois (decisão 0012). Quem usa (a sprint gerada, a pergunta
// livre) monta o pedido e confere o que volta.
package claudecode

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/segredos"
)

// Executavel é o nome procurado no PATH.
const Executavel = "claude"

// SemClaude é o motivo quando ele não está instalado.
const SemClaude = "o Claude Code não está instalado"

// Do que a ferramenta escreve na saída, o resto é descartado.
const maxSaida = 1 << 20

// Argumentos: impressão sem terminal, saída em JSON, nenhuma ferramenta,
// nenhum servidor MCP, nenhuma configuração de projeto ou de usuário (sem
// ganchos) e nenhuma conversa guardada. O pedido vai pela entrada padrão.
var Argumentos = []string{
	"-p", "--output-format", "json", "--tools", "", "--strict-mcp-config",
	"--setting-sources", "", "--no-session-persistence",
}

// Pedido é um pedido ao Claude Code. Env leva a conta do perfil
// (CLAUDE_CONFIG_DIR); Pasta é o começo do nome da pasta vazia.
type Pedido struct {
	Executavel string
	Env        []string
	Pasta      string
	Prazo      time.Duration
}

// Rodar manda a entrada e devolve o texto do `result`, já conferido o envelope.
func (p Pedido) Rodar(ctx context.Context, entrada []byte) (string, error) {
	// Uma pasta vazia: o agente não vê nenhum projeto nem instrução de projeto.
	pasta, err := os.MkdirTemp("", p.Pasta)
	if err != nil {
		return "", err
	}
	defer os.RemoveAll(pasta)
	ctx, cancelar := context.WithTimeout(ctx, p.Prazo)
	defer cancelar()
	cmd := exec.CommandContext(ctx, p.Executavel, Argumentos...)
	cmd.Dir = pasta
	cmd.Env = append(os.Environ(), p.Env...)
	cmd.Stdin = bytes.NewReader(entrada)
	saida := &limitado{max: maxSaida}
	cmd.Stdout = saida
	cmd.Stderr = io.Discard
	cmd.WaitDelay = 2 * time.Second
	err = cmd.Run()
	if errors.Is(ctx.Err(), context.DeadlineExceeded) {
		return "", dados.ErrInvalido{Motivo: fmt.Sprintf("o Claude Code não respondeu em %s", Duracao(p.Prazo))}
	}
	if ctx.Err() != nil {
		return "", ctx.Err()
	}
	if saida.passou {
		return "", dados.ErrInvalido{Motivo: "a resposta do Claude Code veio grande demais"}
	}
	// Com erro, a saída em JSON ainda explica (is_error); sem saída, só o código.
	if err != nil && saida.b.Len() == 0 {
		return "", dados.ErrInvalido{Motivo: "o Claude Code parou com erro sem responder"}
	}
	return Resultado(saida.b.Bytes())
}

// Resultado confere o envelope do --output-format json e devolve o `result`.
func Resultado(saida []byte) (string, error) {
	var envelope struct {
		Tipo    string  `json:"type"`
		Subtipo string  `json:"subtype"`
		Erro    bool    `json:"is_error"`
		Result  *string `json:"result"`
	}
	if err := json.Unmarshal(saida, &envelope); err != nil || envelope.Tipo != "result" {
		return "", dados.ErrInvalido{Motivo: "o Claude Code respondeu num formato que não conheço"}
	}
	if envelope.Erro || envelope.Subtipo != "success" || envelope.Result == nil {
		motivo := "o Claude Code não conseguiu responder"
		if envelope.Result != nil && *envelope.Result != "" && !segredos.Parece(*envelope.Result) {
			motivo += ": " + UmaLinha(*envelope.Result, 160)
		}
		return "", dados.ErrInvalido{Motivo: motivo}
	}
	return *envelope.Result, nil
}

// Controle diz se o texto tem caractere de controle (a quebra de linha só
// vale onde `linhas`).
func Controle(texto string, linhas bool) bool {
	return !utf8.ValidString(texto) || strings.ContainsFunc(texto, func(r rune) bool {
		return (r < ' ' && !(linhas && r == '\n')) || r == 0x7f
	})
}

// UmaLinha fica com a primeira linha, cortada em `maximo` letras.
func UmaLinha(texto string, maximo int) string {
	texto, _, _ = strings.Cut(strings.TrimSpace(texto), "\n")
	if utf8.RuneCountInString(texto) > maximo {
		texto = string([]rune(texto)[:maximo-1]) + "…"
	}
	return texto
}

// Duracao escreve o prazo para quem lê ("3 minutos", "1 minuto", "45s").
func Duracao(d time.Duration) string {
	if d >= time.Minute && d%time.Minute == 0 {
		if d == time.Minute {
			return "1 minuto"
		}
		return fmt.Sprintf("%d minutos", int(d/time.Minute))
	}
	return d.String()
}

// limitado guarda até `max` bytes e marca se passou.
type limitado struct {
	b      bytes.Buffer
	max    int
	passou bool
}

func (l *limitado) Write(p []byte) (int, error) {
	if l.b.Len()+len(p) > l.max {
		l.passou = true
		l.b.Write(p[:max(0, l.max-l.b.Len())])
		return len(p), nil
	}
	return l.b.Write(p)
}
