// Package uso lê o que a statusLine do Claude Code recebe (o contexto da
// conversa e os limites da conta) e tira só o que a Colmeia mostra. O resto
// do JSON (pasta, modelo, conversa) nunca sai daqui.
package uso

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math"
	"strings"
)

// MaxEntrada é o maior JSON da statusLine aceito (ele tem poucos KB).
const MaxEntrada = 256 << 10

// Uso é o que vai ao núcleo: o contexto do agente e os dois limites da conta.
// Cada parte só vem se o Claude Code a informou.
type Uso struct {
	Contexto   *Contexto `json:"contexto,omitempty"`
	CincoHoras *Limite   `json:"cinco_horas,omitempty"`
	Semana     *Limite   `json:"semana,omitempty"`
}

// Contexto: tokens na janela de contexto, o tamanho dela e a porcentagem (0 a 100).
type Contexto struct {
	Usado       int64   `json:"usado"`
	Tamanho     int64   `json:"tamanho"`
	Porcentagem float64 `json:"porcentagem"`
}

// Limite: quanto da janela (5 horas ou semana) já foi, de 0 a 100, e quando
// ela volta (segundos Unix).
type Limite struct {
	Porcentagem float64 `json:"porcentagem"`
	Volta       int64   `json:"volta"`
}

// Os campos da statusLine do Claude Code que interessam (conferidos na
// versão 2.1 e na documentação dela). Os outros são ignorados.
type entrada struct {
	ContextWindow *struct {
		TotalInputTokens  *float64 `json:"total_input_tokens"`
		ContextWindowSize *float64 `json:"context_window_size"`
		UsedPercentage    *float64 `json:"used_percentage"`
	} `json:"context_window"`
	RateLimits *struct {
		FiveHour *limiteEntrada `json:"five_hour"`
		SevenDay *limiteEntrada `json:"seven_day"`
	} `json:"rate_limits"`
}

type limiteEntrada struct {
	UsedPercentage *float64 `json:"used_percentage"`
	ResetsAt       *float64 `json:"resets_at"`
}

// Ler tira o uso do JSON da statusLine.
func Ler(r io.Reader) (Uso, error) {
	bruto, err := io.ReadAll(io.LimitReader(r, MaxEntrada+1))
	if err != nil {
		return Uso{}, err
	}
	if len(bruto) > MaxEntrada {
		return Uso{}, errors.New("entrada grande demais")
	}
	var e entrada
	if err := json.Unmarshal(bruto, &e); err != nil {
		return Uso{}, errors.New("entrada não é JSON")
	}
	var u Uso
	// used_percentage é null antes da primeira mensagem: ainda não há contexto.
	if c := e.ContextWindow; c != nil && c.UsedPercentage != nil && c.ContextWindowSize != nil && *c.ContextWindowSize > 0 {
		u.Contexto = &Contexto{Usado: inteiro(c.TotalInputTokens), Tamanho: inteiro(c.ContextWindowSize), Porcentagem: porcentagem(*c.UsedPercentage)}
	}
	if l := e.RateLimits; l != nil {
		u.CincoHoras, u.Semana = limite(l.FiveHour), limite(l.SevenDay)
	}
	return u, nil
}

func limite(l *limiteEntrada) *Limite {
	if l == nil || l.UsedPercentage == nil || l.ResetsAt == nil || *l.ResetsAt <= 0 {
		return nil
	}
	return &Limite{Porcentagem: porcentagem(*l.UsedPercentage), Volta: inteiro(l.ResetsAt)}
}

func inteiro(v *float64) int64 {
	if v == nil || *v < 0 || *v > 1e15 {
		return 0
	}
	return int64(*v)
}

// porcentagem fica entre 0 e 100, com uma casa.
func porcentagem(v float64) float64 {
	return math.Round(min(max(v, 0), 100)*10) / 10
}

// Maior tamanho de contexto e maior prazo de volta aceitos (o núcleo confere).
const (
	maxTamanho = 100_000_000
	maxVolta   = 1 << 40
)

// Validar confere o que chega ao núcleo: números nos limites, nada mais.
func (u Uso) Validar() error {
	if c := u.Contexto; c != nil {
		if c.Tamanho <= 0 || c.Tamanho > maxTamanho || c.Usado < 0 || c.Usado > maxTamanho || !valida(c.Porcentagem) {
			return errors.New("contexto fora dos limites")
		}
	}
	for _, l := range []*Limite{u.CincoHoras, u.Semana} {
		if l != nil && (!valida(l.Porcentagem) || l.Volta <= 0 || l.Volta > maxVolta) {
			return errors.New("limite fora dos limites")
		}
	}
	return nil
}

func valida(p float64) bool { return p >= 0 && p <= 100 }

// Linha é o que a statusLine imprime no terminal do agente: curta.
func (u Uso) Linha() string {
	partes := []string{"Colmeia"}
	if u.Contexto != nil {
		partes = append(partes, fmt.Sprintf("contexto %.0f%%", u.Contexto.Porcentagem))
	}
	if u.CincoHoras != nil {
		partes = append(partes, fmt.Sprintf("5 h %.0f%%", u.CincoHoras.Porcentagem))
	}
	if u.Semana != nil {
		partes = append(partes, fmt.Sprintf("semana %.0f%%", u.Semana.Porcentagem))
	}
	return strings.Join(partes, " · ")
}
