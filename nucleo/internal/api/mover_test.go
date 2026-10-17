//go:build unix

package api

import (
	"encoding/json"
	"net/http"
	"strings"
	"testing"
)

func TestAgenteMoveAPropriaTarefaSoAPedido(t *testing.T) {
	claudeQueAnota(t)
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	eventos := ouvirComo(t, tokenTela, srv.URL, "1")
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	token := lerArquivo(servidor.DirCanal + "/agentes/1.token")

	// Só Revisão e Concluído, só a tarefa do agente e só com o token dele.
	for _, ruim := range []map[string]any{{"coluna": "trabalhando"}, {"coluna": "backlog"}, {"coluna": "aguardando"}, {"coluna": "concluido", "tarefa": 2}, {"coluna": "revisao", "x": 1}} {
		if status, _ := pedirComo(t, token, "POST", srv.URL+"/v1/agente/coluna", ruim); status != http.StatusBadRequest {
			t.Errorf("aceitou %v: %d", ruim, status)
		}
	}
	if status, _ := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/agente/coluna", map[string]any{"coluna": "revisao"}); status != http.StatusUnauthorized {
		t.Errorf("sem token de agente: %d", status)
	}
	if _, tarefa := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/tarefas/2", nil); tarefa["coluna"] == "concluido" {
		t.Fatal("moveu a tarefa de outro")
	}

	status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/coluna", map[string]any{"coluna": "revisao"})
	if status != http.StatusOK || r["coluna"] != "revisao" {
		t.Fatalf("mover: %d %v", status, r)
	}
	m := eventos.esperar("tarefa movida", func(m map[string]any) bool { return m["tipo"] == "tarefa.atualizada" && m["origem"] == "agente" })
	if tarefa, _ := m["tarefa"].(map[string]any); tarefa["coluna"] != "revisao" || tarefa["coluna_auto"] == true {
		t.Errorf("vale como movimento da usuária: %v", m["tarefa"])
	}
	// A frase da reunião é pedida como em qualquer ida para Revisão.
	eventos.esperar("escrevendo a frase", frase(true))

	_, linha := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/linha-do-tempo", nil)
	texto, _ := json.Marshal(linha)
	if !strings.Contains(string(texto), "foi para Revisão, movida pelo agente a pedido.") {
		t.Errorf("linha do tempo: %s", texto)
	}
}
