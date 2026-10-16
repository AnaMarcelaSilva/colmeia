//go:build unix

package api

import (
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// claudeQueResponde põe no PATH um "claude" que responde `resultado` no envelope
// do --output-format json.
func claudeQueResponde(t *testing.T, resultado string) {
	t.Helper()
	dir := t.TempDir()
	envelope, _ := json.Marshal(map[string]any{"type": "result", "subtype": "success", "is_error": false, "result": resultado})
	if err := os.WriteFile(filepath.Join(dir, "resposta"), envelope, 0o600); err != nil {
		t.Fatal(err)
	}
	script := "#!/bin/sh\ncat > /dev/null\ncat \"" + filepath.Join(dir, "resposta") + "\"\n"
	if err := os.WriteFile(filepath.Join(dir, "claude"), []byte(script), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir+":/usr/bin:/bin")
}

func TestGerarSprintComIA(t *testing.T) {
	srv := servidorComDados(t)
	perfilComAgente(t, srv.URL)
	_, r := pedir(t, "GET", srv.URL+"/v1/perfis/1/sprints", nil)
	sprint := int64(r["atual"].(float64))
	gerar := fmt.Sprintf("%s/v1/perfis/1/sprints/gerar?sprint=%d", srv.URL, sprint)
	deck := func() map[string]any {
		_, d := pedir(t, "GET", fmt.Sprintf("%s/v1/perfis/1/apresentacao?tipo=sprint&sprint=%d", srv.URL, sprint), nil)
		return d["slides"].([]any)[0].(map[string]any)
	}

	// Sem o Claude Code instalado, o núcleo explica.
	t.Setenv("PATH", t.TempDir()+":/usr/bin:/bin")
	if status, r := pedir(t, "POST", gerar, map[string]string{"pedido": "agrupe por cliente"}); status != http.StatusBadRequest || !strings.Contains(fmt.Sprint(r["erro"]), "não está instalado") {
		t.Fatalf("sem Claude Code: %d %v", status, r)
	}

	// Uma proposta que não serve não guarda nada.
	claudeQueResponde(t, `{"itens":[{"tarefa_id":99,"titulo":"x","descricao":"y","assunto":""}]}`)
	if status, r := pedir(t, "POST", gerar, map[string]string{"pedido": "agrupe por cliente"}); status != http.StatusBadRequest || !strings.Contains(fmt.Sprint(r["erro"]), "não é desta sprint") {
		t.Fatalf("proposta ruim: %d %v", status, r)
	}
	if status, _ := pedir(t, "POST", gerar, map[string]string{"pedido": ""}); status != http.StatusBadRequest {
		t.Errorf("pedido vazio: %d", status)
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/perfis/1/sprints/gerar", map[string]string{"pedido": "x"}); status != http.StatusBadRequest {
		t.Errorf("sem sprint: %d", status)
	}

	// A proposta vem com o antes e o depois; nada muda até aceitar.
	claudeQueResponde(t, `{"itens":[{"tarefa_id":1,"titulo":"Relatório para o cliente","descricao":"O relatório ficou pronto e o cliente já usa.","assunto":"Cliente X"}]}`)
	status, p := pedir(t, "POST", gerar, map[string]string{"pedido": "agrupe por cliente e escreva para o gerente"})
	if status != http.StatusOK {
		t.Fatalf("gerar: %d %v", status, p)
	}
	item := p["itens"].([]any)[0].(map[string]any)
	antes := item["antes"].(map[string]any)
	if item["titulo"] != "Relatório para o cliente" || item["assunto"] != "Cliente X" || antes["titulo"] != "Analisar relatório" || item["projeto"] != "cliente-x" {
		t.Fatalf("proposta: %v", p)
	}
	if s := deck(); s["titulo"] != "Analisar relatório" || s["nota"] != "" || s["assunto_id"] != nil {
		t.Fatalf("mudou antes de aceitar: %v", s)
	}
	id := int64(p["id"].(float64))
	if status, r := pedir(t, "POST", fmt.Sprintf("%s/v1/propostas/%d/aceitar", srv.URL, id), nil); status != http.StatusNoContent {
		t.Fatalf("aceitar: %d %v", status, r)
	}
	s := deck()
	if s["titulo"] != "Relatório para o cliente" || s["titulo_original"] != "Analisar relatório" || s["nota"] != "O relatório ficou pronto e o cliente já usa." || s["secao"] != "Cliente X" {
		t.Errorf("depois de aceitar: %v", s)
	}
	// Aceita uma vez só; descartar o que não existe dá 404.
	if status, _ := pedir(t, "POST", fmt.Sprintf("%s/v1/propostas/%d/aceitar", srv.URL, id), nil); status != http.StatusBadRequest {
		t.Errorf("aceitar de novo: %d", status)
	}
	if status, _ := pedir(t, "DELETE", fmt.Sprintf("%s/v1/propostas/%d", srv.URL, id), nil); status != http.StatusNotFound {
		t.Errorf("descartar a aceita: %d", status)
	}

	// Descartar não grava nada; o mesmo título da tarefa apaga o da sprint.
	claudeQueResponde(t, `{"itens":[{"tarefa_id":1,"titulo":"Analisar relatório","descricao":"Outra frase.","assunto":"Cliente X"}]}`)
	_, p = pedir(t, "POST", gerar, map[string]string{"pedido": "volte o título"})
	id = int64(p["id"].(float64))
	if status, _ := pedir(t, "DELETE", fmt.Sprintf("%s/v1/propostas/%d", srv.URL, id), nil); status != http.StatusNoContent {
		t.Errorf("descartar: %d", status)
	}
	if s := deck(); s["nota"] != "O relatório ficou pronto e o cliente já usa." {
		t.Errorf("descartar gravou: %v", s)
	}
	_, p = pedir(t, "POST", gerar, map[string]string{"pedido": "volte o título"})
	pedir(t, "POST", fmt.Sprintf("%s/v1/propostas/%d/aceitar", srv.URL, int64(p["id"].(float64))), nil)
	if s := deck(); s["titulo"] != "Analisar relatório" || s["titulo_original"] != nil || s["secao"] != "Cliente X" {
		t.Errorf("título de volta e o mesmo assunto: %v", s)
	}
	_, d := pedir(t, "GET", fmt.Sprintf("%s/v1/perfis/1/apresentacao?tipo=sprint&sprint=%d", srv.URL, sprint), nil)
	if n := len(d["assuntos"].([]any)); n != 1 {
		t.Errorf("o assunto repetido não pode duplicar: %d", n)
	}
}
