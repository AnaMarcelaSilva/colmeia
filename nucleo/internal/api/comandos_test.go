//go:build unix

package api

import (
	"context"
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"
)

func tipo(t string) func(map[string]any) bool {
	return func(m map[string]any) bool { return m["tipo"] == t }
}

func TestPlayDoProjeto(t *testing.T) {
	srv := servidorComDados(t)
	pasta := t.TempDir()
	os.MkdirAll(filepath.Join(pasta, "api"), 0o755)
	os.WriteFile(filepath.Join(pasta, "package.json"), []byte(`{"scripts":{"dev":"vite"}}`), 0o644)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Estudo"})
	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "pessoal"})
	if status, p := pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "loja", "caminho": pasta}); status != http.StatusOK {
		t.Fatalf("projeto: %d %v", status, p)
	}
	eventos := ouvir(t, srv.URL, "1")

	// Sugestões: o script do package.json.
	status, sug := pedir(t, "GET", srv.URL+"/v1/projetos/1/comandos/sugestoes", nil)
	lista, _ := sug["sugestoes"].([]any)
	if status != http.StatusOK || len(lista) != 1 || lista[0].(map[string]any)["comando"] != "npm run dev" {
		t.Fatalf("sugestões: %d %v", status, sug)
	}

	// Validação: pasta fora do projeto, nome vazio, variável inválida.
	for _, ruim := range []map[string]any{
		{"nome": "x", "comando": "ls", "pasta": "../fora"},
		{"nome": "x", "comando": "ls", "pasta": "/etc"},
		{"nome": " ", "comando": "ls"},
		{"nome": "x", "comando": ""},
		{"nome": "x", "comando": "ls", "ambiente": []map[string]string{{"nome": "A B", "valor": "1"}}},
	} {
		if status, r := pedir(t, "POST", srv.URL+"/v1/projetos/1/comandos", ruim); status != http.StatusBadRequest {
			t.Errorf("%v: %d %v", ruim, status, r)
		}
	}

	status, c := pedir(t, "POST", srv.URL+"/v1/projetos/1/comandos", map[string]any{
		"nome": "saida", "comando": `echo "oi $NOME em $(basename "$PWD")"; exit 3`, "pasta": "api",
		"ambiente": []map[string]string{{"nome": "NOME", "valor": "colmeia"}},
	})
	if status != http.StatusOK || c["id"] != float64(1) || c["rodando"] != false || c["origem"] != "voce" {
		t.Fatalf("criar: %d %v", status, c)
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/projetos/1/comandos", map[string]any{"nome": "saida", "comando": "ls"}); status != http.StatusBadRequest {
		t.Errorf("nome repetido: %d", status)
	}
	// O terminal só existe depois de rodar.
	if status, _ := pedir(t, "GET", srv.URL+"/v1/comandos/1/terminal", nil); status != http.StatusNotFound {
		t.Errorf("terminal antes de rodar: %d", status)
	}

	if status, r := pedir(t, "POST", srv.URL+"/v1/comandos/1/rodar", map[string]any{"cols": 100, "rows": 30}); status != http.StatusOK || r["rodando"] != true {
		t.Fatalf("rodar: %d %v", status, r)
	}
	inicio := eventos.esperar("início", tipo("comando.iniciou"))
	if inicio["comando_id"] != float64(1) || !strings.HasSuffix(inicio["onde"].(string), "api") {
		t.Errorf("início: %v", inicio)
	}
	fim := eventos.esperar("fim", tipo("comando.terminou"))
	if fim["codigo"] != float64(3) || fim["parada"] != false {
		t.Errorf("fim: %v", fim)
	}
	// A saída fica no terminal, para ver depois.
	ctx, cancelar := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancelar()
	term, _, err := websocket.Dial(ctx, strings.Replace(srv.URL, "http", "ws", 1)+"/v1/comandos/1/terminal", nil)
	if err != nil {
		t.Fatal(err)
	}
	defer term.CloseNow()
	var saida string
	for !strings.Contains(saida, "oi colmeia em api") {
		_, b, err := term.Read(ctx)
		if err != nil {
			t.Fatalf("a saída não chegou ao terminal: %q", saida)
		}
		saida += string(b)
	}
	// A lista diz como terminou a última vez.
	r, _ := http.Get(srv.URL + "/v1/projetos/1/comandos")
	var lista2 []map[string]any
	json.NewDecoder(r.Body).Decode(&lista2)
	r.Body.Close()
	if len(lista2) != 1 || lista2[0]["rodando"] != false || lista2[0]["fim"].(map[string]any)["codigo"] != float64(3) {
		t.Errorf("lista depois de rodar: %v", lista2)
	}

	// Parar uma execução longa: o fim diz que foi a Colmeia.
	pedir(t, "PATCH", srv.URL+"/v1/comandos/1", map[string]any{"nome": "longo", "comando": "sleep 30"})
	pedir(t, "POST", srv.URL+"/v1/comandos/1/rodar", nil)
	eventos.esperar("início", tipo("comando.iniciou"))
	if status, _ := pedir(t, "POST", srv.URL+"/v1/comandos/1/parar", nil); status != http.StatusNoContent {
		t.Errorf("parar: %d", status)
	}
	if fim := eventos.esperar("fim", tipo("comando.terminou")); fim["parada"] != true {
		t.Errorf("fim parado: %v", fim)
	}

	// Rodar numa tarefa de outro projeto não pode; remover apaga.
	if status, _ := pedir(t, "POST", srv.URL+"/v1/comandos/1/rodar", map[string]any{"tarefa_id": 99}); status != http.StatusNotFound {
		t.Errorf("tarefa inexistente: %d", status)
	}
	if status, _ := pedir(t, "DELETE", srv.URL+"/v1/comandos/1", nil); status != http.StatusNoContent {
		t.Errorf("remover: %d", status)
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/comandos/1/rodar", nil); status != http.StatusNotFound {
		t.Errorf("rodar removida: %d", status)
	}
}
