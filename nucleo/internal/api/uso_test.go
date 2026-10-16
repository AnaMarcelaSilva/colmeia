//go:build unix

package api

import (
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestStatusLineMandaOUsoDoAgente(t *testing.T) {
	argumentos, _ := claudeQueAnota(t)
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	esperarAte(t, "os argumentos do claude", func() bool { return strings.Contains(lerArquivo(argumentos), "--settings") })

	// Os settings da sessão: só a statusLine, chamando o próprio núcleo com o
	// arquivo do token (nunca o token), num arquivo 0600.
	dir := filepath.Join(servidor.DirCanal, "agentes")
	arquivo := filepath.Join(dir, "1.settings.json")
	if !strings.Contains(lerArquivo(argumentos), "--settings "+arquivo) {
		t.Errorf("argumentos: %s", lerArquivo(argumentos))
	}
	if info, err := os.Stat(arquivo); err != nil || info.Mode().Perm() != 0o600 {
		t.Fatalf("settings: %v", err)
	}
	var settings struct {
		StatusLine struct{ Type, Command string } `json:"statusLine"`
	}
	json.Unmarshal([]byte(lerArquivo(arquivo)), &settings)
	token := lerArquivo(filepath.Join(dir, "1.token"))
	comando := settings.StatusLine.Command
	if settings.StatusLine.Type != "command" || !strings.HasPrefix(comando, "'/bin/true' uso --socket '") ||
		!strings.Contains(comando, "--token-arquivo '"+filepath.Join(dir, "1.token")+"'") || strings.Contains(comando, token) {
		t.Errorf("statusLine: %+v", settings.StatusLine)
	}

	eventos := ouvirComo(t, tokenTela, srv.URL, "1")
	volta := time.Now().Add(2 * time.Hour).Unix()
	uso := map[string]any{"contexto": map[string]any{"usado": 84000, "tamanho": 200000, "porcentagem": 42},
		"cinco_horas": map[string]any{"porcentagem": 12.5, "volta": volta}}
	// Só com o token do agente, e com JSON estrito.
	if status, _ := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/agente/uso", uso); status != http.StatusUnauthorized {
		t.Errorf("com o token da tela: %d", status)
	}
	if status, _ := pedirComo(t, token, "POST", srv.URL+"/v1/agente/uso", map[string]any{"cwd": "/home"}); status != http.StatusBadRequest {
		t.Errorf("campo desconhecido: %d", status)
	}
	if status, _ := pedirComo(t, token, "POST", srv.URL+"/v1/agente/uso", map[string]any{"contexto": map[string]any{"usado": 1, "tamanho": 10, "porcentagem": 300}}); status != http.StatusBadRequest {
		t.Errorf("porcentagem fora: %d", status)
	}
	if status, _ := pedirComo(t, token, "POST", srv.URL+"/v1/agente/uso", uso); status != http.StatusNoContent {
		t.Fatalf("uso: %d", status)
	}
	m := eventos.esperar("agente.uso", func(m map[string]any) bool { return m["tipo"] == "agente.uso" })
	contexto, _ := m["contexto"].(map[string]any)
	cinco, _ := m["cinco_horas"].(map[string]any)
	if m["agente_id"] != 1.0 || m["tarefa_id"] != 1.0 || contexto["porcentagem"] != 42.0 || cinco["porcentagem"] != 12.5 ||
		!strings.HasPrefix(cinco["volta_texto"].(string), "volta às ") || m["semana"] != nil {
		t.Errorf("evento: %v", m)
	}
	// O mesmo uso de novo não vira evento; o retrato do quadro já traz o uso.
	pedirComo(t, token, "POST", srv.URL+"/v1/agente/uso", uso)
	_, quadro := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/quadro", nil)
	doPerfil, _ := quadro["uso"].(map[string]any)
	if agentes, _ := doPerfil["agentes"].([]any); len(agentes) != 1 || doPerfil["cinco_horas"] == nil {
		t.Errorf("uso no quadro: %v", quadro["uso"])
	}

	// Um agente novo ainda sem os limites não apaga os da conta.
	pedirComo(t, token, "POST", srv.URL+"/v1/agente/uso", map[string]any{})
	m = eventos.esperar("agente.uso sem contexto", func(m map[string]any) bool { return m["tipo"] == "agente.uso" })
	if m["contexto"] != nil || m["cinco_horas"] == nil {
		t.Errorf("sem contexto: %v", m)
	}

	// O agente removido: o token não vale mais e o uso dele some.
	pedirComo(t, tokenTela, "DELETE", srv.URL+"/v1/agentes/1", nil)
	if status, _ := pedirComo(t, token, "POST", srv.URL+"/v1/agente/uso", uso); status != http.StatusUnauthorized {
		t.Errorf("token de agente removido: %d", status)
	}
	if _, err := os.Stat(arquivo); !os.IsNotExist(err) {
		t.Error("os settings do agente removido ficaram")
	}
}

func TestTextoDaVolta(t *testing.T) {
	agora := time.Date(2026, 10, 16, 10, 0, 0, 0, time.Local)
	if s := textoDaVolta(agora.Add(3*time.Hour), agora); s != "volta às 13:00" {
		t.Errorf("mesmo dia: %q", s)
	}
	if s := textoDaVolta(agora.Add(50*time.Hour), agora); s != "volta às 12:00 de 18/10" {
		t.Errorf("outro dia: %q", s)
	}
	if paraTela(nil, agora) != nil {
		t.Error("sem limite")
	}
}
