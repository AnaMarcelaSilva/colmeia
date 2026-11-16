//go:build unix

package api

import (
	"fmt"
	"net/http"
	"strings"
	"testing"
)

func TestForaDaSprintETextoDoAssunto(t *testing.T) {
	srv := servidorComDados(t)
	perfilComAgente(t, srv.URL)
	_, r := pedir(t, "GET", srv.URL+"/v1/perfis/1/sprints", nil)
	sprint := int64(r["atual"].(float64))
	deckURL := fmt.Sprintf("%s/v1/perfis/1/apresentacao?tipo=sprint&sprint=%d", srv.URL, sprint)
	resumoURL := fmt.Sprintf("%s/v1/perfis/1/resumo?tipo=sprint&sprint=%d", srv.URL, sprint)

	// Um assunto com a tarefa dentro.
	status, a := pedir(t, "POST", fmt.Sprintf("%s/v1/sprints/%d/assuntos", srv.URL, sprint), map[string]string{"nome": "Loja"})
	if status != http.StatusOK || a["texto"] != "" {
		t.Fatalf("assunto: %d %v", status, a)
	}
	assunto := int64(a["id"].(float64))
	if status, _ := pedir(t, "PUT", fmt.Sprintf("%s/v1/sprints/%d/assuntos/tarefas", srv.URL, sprint), map[string]any{"tarefas": []int64{1}, "assunto": assunto}); status != http.StatusNoContent {
		t.Fatalf("tarefa no assunto: %d", status)
	}

	// A fala escrita por você vai ao deck e ao texto do resumo.
	status, a = pedir(t, "PUT", fmt.Sprintf("%s/v1/assuntos/%d/texto", srv.URL, assunto), map[string]string{"texto": " Fechamos a entrada das lojas. "})
	if status != http.StatusOK || a["texto"] != "Fechamos a entrada das lojas." || a["texto_origem"] != "voce" {
		t.Fatalf("texto escrito: %d %v", status, a)
	}
	_, deck := pedir(t, "GET", deckURL, nil)
	if as := deck["assuntos"].([]any)[0].(map[string]any); as["texto"] != "Fechamos a entrada das lojas." {
		t.Fatalf("texto no deck: %v", as)
	}
	_, resumo := pedir(t, "GET", resumoURL, nil)
	if md := fmt.Sprint(resumo["markdown"]); !strings.Contains(md, "## Loja\n\nFechamos a entrada das lojas.") {
		t.Fatalf("texto no markdown: %q", md)
	}
	if status, _ := pedir(t, "PUT", fmt.Sprintf("%s/v1/assuntos/%d/texto", srv.URL, assunto), map[string]string{"texto": strings.Repeat("a", 4001)}); status != http.StatusBadRequest {
		t.Fatalf("texto comprido: %d", status)
	}
	if status, _ := pedir(t, "PUT", srv.URL+"/v1/assuntos/99/texto", map[string]string{"texto": "x"}); status != http.StatusNotFound {
		t.Fatalf("assunto que não existe: %d", status)
	}

	// Gerado pela IA: o texto limpo, a origem e com quantas tarefas.
	claudeQueResponde(t, "```\nA loja ganhou a entrada nova.\n```")
	status, a = pedir(t, "POST", fmt.Sprintf("%s/v1/assuntos/%d/texto/gerar", srv.URL, assunto), map[string]string{})
	if status != http.StatusOK || a["texto"] != "A loja ganhou a entrada nova." || a["texto_origem"] != "ia" || a["texto_tarefas"] != float64(1) {
		t.Fatalf("texto gerado: %d %v", status, a)
	}

	// Tirar a tarefa da sprint: ela sai do deck e do resumo, fica na lista
	// de fora e na daily.
	eventos := ouvirComo(t, "", srv.URL, "1")
	if status, _ := pedir(t, "PUT", fmt.Sprintf("%s/v1/sprints/%d/fora/1", srv.URL, sprint), map[string]bool{"fora": true}); status != http.StatusNoContent {
		t.Fatalf("tirar: %d", status)
	}
	e := eventos.esperar("sprint.fora", func(e map[string]any) bool { return e["tipo"] == "sprint.fora" })
	if e["tarefa_id"] != float64(1) || e["sprint_id"] != float64(sprint) || e["fora"] != true {
		t.Fatalf("evento: %v", e)
	}
	_, deck = pedir(t, "GET", deckURL, nil)
	if n := len(deck["slides"].([]any)); n != 0 {
		t.Fatalf("a tarefa continua no deck: %v", deck)
	}
	fora := deck["fora"].([]any)
	if len(fora) != 1 || fora[0].(map[string]any)["titulo"] != "Analisar relatório" {
		t.Fatalf("fora: %v", fora)
	}
	_, resumo = pedir(t, "GET", resumoURL, nil)
	if md := fmt.Sprint(resumo["markdown"]); strings.Contains(md, "Analisar relatório") {
		t.Fatalf("a tarefa continua no resumo: %q", md)
	}
	_, daily := pedir(t, "GET", srv.URL+"/v1/perfis/1/apresentacao?tipo=daily", nil)
	if n := len(daily["slides"].([]any)); n != 1 {
		t.Fatalf("a daily mudou: %v", daily)
	}
	// Sem tarefas no assunto, não há o que gerar.
	if status, r := pedir(t, "POST", fmt.Sprintf("%s/v1/assuntos/%d/texto/gerar", srv.URL, assunto), map[string]string{}); status != http.StatusBadRequest || !strings.Contains(fmt.Sprint(r["erro"]), "não tem tarefas") {
		t.Fatalf("gerar sem tarefas: %d %v", status, r)
	}
	// Tarefa ou sprint que não existem; de volta à sprint.
	if status, _ := pedir(t, "PUT", fmt.Sprintf("%s/v1/sprints/%d/fora/99", srv.URL, sprint), map[string]bool{"fora": true}); status != http.StatusNotFound {
		t.Fatalf("tarefa que não existe: %d", status)
	}
	if status, _ := pedir(t, "PUT", srv.URL+"/v1/sprints/99/fora/1", map[string]bool{"fora": true}); status != http.StatusNotFound {
		t.Fatalf("sprint que não existe: %d", status)
	}
	if status, _ := pedir(t, "PUT", fmt.Sprintf("%s/v1/sprints/%d/fora/1", srv.URL, sprint), map[string]bool{"fora": false}); status != http.StatusNoContent {
		t.Fatalf("trazer de volta: %d", status)
	}
	_, deck = pedir(t, "GET", deckURL, nil)
	if n := len(deck["slides"].([]any)); n != 1 || deck["fora"] != nil {
		t.Fatalf("de volta: %v", deck)
	}
	// Texto vazio apaga a fala.
	if _, a := pedir(t, "PUT", fmt.Sprintf("%s/v1/assuntos/%d/texto", srv.URL, assunto), map[string]string{"texto": ""}); a["texto"] != "" || a["texto_origem"] != nil {
		t.Fatalf("apagar o texto: %v", a)
	}
}
