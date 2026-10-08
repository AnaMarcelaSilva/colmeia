//go:build unix

package api

import (
	"fmt"
	"net/http"
	"testing"
	"time"
)

func TestSprintsComTitulos(t *testing.T) {
	srv := servidorComDados(t)
	perfilComAgente(t, srv.URL)

	// A atual nasce na primeira consulta e tem hoje.
	status, r := pedir(t, "GET", srv.URL+"/v1/perfis/1/sprints", nil)
	if status != http.StatusOK {
		t.Fatalf("listar: %d %v", status, r)
	}
	sprints := r["sprints"].([]any)
	atual := sprints[0].(map[string]any)
	hoje := time.Now().Format("2006-01-02")
	if len(sprints) != 1 || r["atual"] != atual["id"] || atual["inicio"].(string) > hoje || atual["fim"].(string) < hoje {
		t.Fatalf("sprint atual: %v", r)
	}
	id := int64(atual["id"].(float64))

	// O título da sprint aparece no deck da sprint, não no da daily.
	if status, r := pedir(t, "PUT", fmt.Sprintf("%s/v1/sprints/%d/titulos/1", srv.URL, id), map[string]string{"titulo": "Relatório do cliente"}); status != http.StatusNoContent {
		t.Fatalf("título: %d %v", status, r)
	}
	if status, _ := pedir(t, "PUT", fmt.Sprintf("%s/v1/sprints/%d/titulos/99", srv.URL, id), map[string]string{"titulo": "x"}); status != http.StatusNotFound {
		t.Errorf("tarefa que não existe: %d", status)
	}
	_, deck := pedir(t, "GET", fmt.Sprintf("%s/v1/perfis/1/apresentacao?tipo=sprint&sprint=%d", srv.URL, id), nil)
	slides := deck["slides"].([]any)
	if len(slides) != 1 || deck["sprint_id"] != float64(id) {
		t.Fatalf("deck da sprint: %v", deck)
	}
	if s := slides[0].(map[string]any); s["titulo"] != "Relatório do cliente" || s["titulo_original"] != "Analisar relatório" {
		t.Errorf("slide: %v", s)
	}
	if chave := deck["chave_nota"]; chave != atual["inicio"].(string)+".."+atual["fim"].(string) {
		t.Errorf("chave das notas: %v", chave)
	}
	if status, _ := pedir(t, "GET", fmt.Sprintf("%s/v1/perfis/1/resumo?tipo=sprint&sprint=%d", srv.URL, id), nil); status != http.StatusOK {
		t.Errorf("resumo da sprint: %d", status)
	}
	_, daily := pedir(t, "GET", srv.URL+"/v1/perfis/1/apresentacao?tipo=daily", nil)
	if s := daily["slides"].([]any)[0].(map[string]any); s["titulo"] != "Analisar relatório" {
		t.Errorf("a daily mudou: %v", s)
	}
	// Sprint de outro perfil (ou que não existe) não serve.
	if status, _ := pedir(t, "GET", srv.URL+"/v1/perfis/1/apresentacao?tipo=sprint&sprint=99", nil); status != http.StatusNotFound {
		t.Errorf("sprint que não existe: %d", status)
	}

	// Datas: editar, não cruzar outra, remover.
	if status, r := pedir(t, "POST", srv.URL+"/v1/perfis/1/sprints", map[string]string{"inicio": atual["inicio"].(string), "fim": atual["fim"].(string)}); status != http.StatusBadRequest {
		t.Errorf("sprint por cima de outra: %d %v", status, r)
	}
	if status, r := pedir(t, "PATCH", fmt.Sprintf("%s/v1/sprints/%d", srv.URL, id), map[string]string{"inicio": "2026-13-01", "fim": "2026-10-08"}); status != http.StatusBadRequest {
		t.Errorf("data inválida: %d %v", status, r)
	}
	if status, _ := pedir(t, "DELETE", fmt.Sprintf("%s/v1/sprints/%d", srv.URL, id), nil); status != http.StatusNoContent {
		t.Errorf("remover: %d", status)
	}
}
