package uso

import (
	"strings"
	"testing"
)

// Um JSON como o da statusLine do Claude Code 2.1, com campos que a Colmeia
// não usa (e que não podem passar adiante).
const exemplo = `{
  "session_id": "abc", "cwd": "/home/x/loja-web", "transcript_path": "/home/x/.claude/t.jsonl",
  "model": {"id": "modelo", "display_name": "Modelo"},
  "context_window": {"total_input_tokens": 84000, "total_output_tokens": 900, "context_window_size": 200000,
    "current_usage": {"input_tokens": 10}, "used_percentage": 42.04, "remaining_percentage": 57.96},
  "rate_limits": {"five_hour": {"used_percentage": 12.5, "resets_at": 1792850400},
    "seven_day": {"used_percentage": 130, "resets_at": 1793200000}, "spend_limit": {"used_percentage": 1}}
}`

func TestLerTiraSoOQueAColmeiaMostra(t *testing.T) {
	u, err := Ler(strings.NewReader(exemplo))
	if err != nil {
		t.Fatal(err)
	}
	if u.Contexto == nil || *u.Contexto != (Contexto{Usado: 84000, Tamanho: 200000, Porcentagem: 42}) {
		t.Errorf("contexto: %+v", u.Contexto)
	}
	if u.CincoHoras == nil || *u.CincoHoras != (Limite{Porcentagem: 12.5, Volta: 1792850400}) {
		t.Errorf("5 h: %+v", u.CincoHoras)
	}
	// Acima de 100 vira 100.
	if u.Semana == nil || u.Semana.Porcentagem != 100 {
		t.Errorf("semana: %+v", u.Semana)
	}
	if err := u.Validar(); err != nil {
		t.Errorf("validar: %v", err)
	}
	if l := u.Linha(); l != "Colmeia · contexto 42% · 5 h 12% · semana 100%" {
		t.Errorf("linha: %q", l)
	}
}

func TestLerSemDados(t *testing.T) {
	// Antes da primeira mensagem: used_percentage null e sem rate_limits.
	u, err := Ler(strings.NewReader(`{"context_window": {"context_window_size": 200000, "used_percentage": null, "current_usage": null}}`))
	if err != nil || u.Contexto != nil || u.CincoHoras != nil || u.Semana != nil {
		t.Errorf("sem dados: %+v %v", u, err)
	}
	if u.Linha() != "Colmeia" {
		t.Errorf("linha: %q", u.Linha())
	}
	if _, err := Ler(strings.NewReader("não é json")); err == nil {
		t.Error("aceitou o que não é JSON")
	}
	if _, err := Ler(strings.NewReader(`{"x":"` + strings.Repeat("a", MaxEntrada) + `"}`)); err == nil {
		t.Error("aceitou uma entrada grande demais")
	}
}

func TestValidar(t *testing.T) {
	ruins := []Uso{
		{Contexto: &Contexto{Usado: 1, Tamanho: 0, Porcentagem: 1}},
		{Contexto: &Contexto{Usado: -1, Tamanho: 10, Porcentagem: 1}},
		{Contexto: &Contexto{Usado: 1, Tamanho: 10, Porcentagem: 101}},
		{CincoHoras: &Limite{Porcentagem: -1, Volta: 10}},
		{Semana: &Limite{Porcentagem: 10, Volta: 0}},
	}
	for _, u := range ruins {
		if u.Validar() == nil {
			t.Errorf("aceitou %+v", u)
		}
	}
	if (Uso{}).Validar() != nil {
		t.Error("recusou o uso vazio")
	}
}
