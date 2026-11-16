package api

import (
	"net/http"
	"testing"
	"time"
)

// A pauta: a ordem que você dá vale dentro da seção, o visto conta na
// reunião e "Daily apresentada" fecha a de hoje: a próxima começa limpa,
// sem a tarefa que já concluiu, e com a que ficou aberta.
func TestPautaOrdemVistoEDailyApresentada(t *testing.T) {
	esperaSimples, esperaMontador := EsperaSimples, EsperaMontador
	EsperaSimples, EsperaMontador = time.Hour, time.Hour
	t.Cleanup(func() { EsperaSimples, EsperaMontador = esperaSimples, esperaMontador })
	srv, _ := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/1", map[string]string{"coluna": "trabalhando"})
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/2", map[string]string{"coluna": "trabalhando"})
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/2", map[string]string{"coluna": "concluido"})

	ids := func() []float64 {
		_, deck := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/apresentacao?tipo=daily", nil)
		var lista []float64
		for _, s := range deck["slides"].([]any) {
			lista = append(lista, s.(map[string]any)["tarefa_id"].(float64))
		}
		return lista
	}
	// A ordem padrão: concluídas primeiro.
	if l := ids(); len(l) != 2 || l[0] != 2 || l[1] != 1 {
		t.Fatalf("ordem padrão: %v", l)
	}
	// A sua ordem.
	status, _ := pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/perfis/1/pauta", map[string]any{"tipo": "daily", "chave": hoje(), "ordem": []int64{1, 2}})
	if status != http.StatusNoContent {
		t.Fatalf("ordenar: %d", status)
	}
	if l := ids(); l[0] != 1 || l[1] != 2 {
		t.Fatalf("ordem sua: %v", l)
	}
	// O visto.
	status, _ = pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/perfis/1/pauta/1", map[string]any{"tipo": "daily", "chave": hoje(), "apresentada": true})
	if status != http.StatusNoContent {
		t.Fatalf("marcar: %d", status)
	}
	_, deck := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/apresentacao?tipo=daily", nil)
	primeiro := deck["slides"].([]any)[0].(map[string]any)
	if deck["apresentadas"] != 1.0 || primeiro["apresentada"] != true || primeiro["ordem"] != 1.0 {
		t.Fatalf("visto: apresentadas=%v slide=%v", deck["apresentadas"], primeiro)
	}
	// Daily apresentada: a de hoje fecha; a próxima vem sem a 2 (concluída e
	// já falada) e com a 1 (segue aberta), sem vistos.
	status, resp := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/perfis/1/daily/apresentada", nil)
	if status != http.StatusOK || resp["em"] == "" {
		t.Fatalf("fechar: %d %v", status, resp)
	}
	_, deck = pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/apresentacao?tipo=daily", nil)
	slides := deck["slides"].([]any)
	if deck["fechada_em"] == "" || deck["apresentadas"] != 0.0 || len(slides) != 1 || slides[0].(map[string]any)["tarefa_id"] != 1.0 || slides[0].(map[string]any)["apresentada"] == true {
		t.Fatalf("depois de fechar: fechada_em=%v apresentadas=%v slides=%v", deck["fechada_em"], deck["apresentadas"], slides)
	}
	// Trabalho novo na 2 depois da daily: ela volta.
	time.Sleep(1100 * time.Millisecond)
	pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/tarefas/2/notas", map[string]string{"tipo": "daily", "periodo": hoje(), "texto": "Ajuste depois da daily."})
	if l := ids(); len(l) != 2 {
		t.Fatalf("depois de trabalho novo: %v", l)
	}
}
