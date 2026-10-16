//go:build unix

package api

import (
	"context"
	"net/http"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"
)

func TestMascoteEAbelhaFixaPorPerfil(t *testing.T) {
	srv := servidorComDados(t)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Pessoal"})
	_, perfis := pedirLista(t, srv.URL+"/v1/perfis")
	for _, p := range perfis {
		if p["mascote_modelo"] != "identidade" || p["mascote_nome"] != "Colmeia" || p["abelha_fixa"] != false {
			t.Fatalf("padrões da abelha: %v", p)
		}
	}
	eventos := ouvir(t, srv.URL, "1")

	if status, r := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"mascote_modelo": "favo", "mascote_nome": "Zuzu"}); status != http.StatusOK {
		t.Fatalf("trocar a abelha: %d %v", status, r)
	}
	if m := eventos.esperar("perfil.mascote", func(m map[string]any) bool { return m["tipo"] == "perfil.mascote" }); m["modelo"] != "favo" || m["nome"] != "Zuzu" || m["perfil_id"] != 1.0 {
		t.Errorf("evento da abelha: %v", m)
	}
	// Só o nome: o modelo continua.
	pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"mascote_nome": "Mel"})
	for _, caso := range []map[string]any{
		{"mascote_modelo": "unicornio"},
		{"mascote_nome": ""},
		{"mascote_nome": strings.Repeat("a", 25)},
		{"mascote_modelo": 3},
		{"abelha_fixa": "sim"},
	} {
		if status, _ := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", caso); status != http.StatusBadRequest {
			t.Errorf("%v: %d", caso, status)
		}
	}
	if status, r := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"abelha_fixa": true}); status != http.StatusOK {
		t.Fatalf("fixar: %d %v", status, r)
	}
	if m := eventos.esperar("perfil.abelha_fixa", func(m map[string]any) bool { return m["tipo"] == "perfil.abelha_fixa" }); m["ligada"] != true {
		t.Errorf("evento da abelha fixa: %v", m)
	}
	_, perfis = pedirLista(t, srv.URL+"/v1/perfis")
	for _, p := range perfis {
		um := p["id"] == 1.0
		if um != (p["mascote_modelo"] == "favo" && p["mascote_nome"] == "Mel" && p["abelha_fixa"] == true) {
			t.Errorf("vale só para o perfil 1: %v", p)
		}
	}
	if status, _ := pedir(t, "PATCH", srv.URL+"/v1/perfis/99", map[string]any{"mascote_nome": "Mel"}); status != http.StatusNotFound {
		t.Errorf("perfil que não existe: %d", status)
	}
}

// ouvirAbelha abre os eventos como a abelha fixa.
func ouvirAbelha(t *testing.T, url, perfil string) *ouvinte {
	t.Helper()
	ctx, cancelar := context.WithTimeout(context.Background(), 15*time.Second)
	t.Cleanup(cancelar)
	conn, _, err := websocket.Dial(ctx, "ws"+strings.TrimPrefix(url, "http")+"/v1/perfis/"+perfil+"/eventos?quem=abelha", nil)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { conn.CloseNow() })
	o := &ouvinte{t, conn, ctx}
	if m := o.proxima(); m["tipo"] != "ola" {
		t.Fatalf("primeira mensagem: %v", m)
	}
	return o
}

func TestAbrirATarefaPelaAbelha(t *testing.T) {
	srv, _ := doisWorkspaces(t)
	pedir(t, "POST", srv.URL+"/v1/workspaces/3/projetos", map[string]string{"nome": "alheio", "caminho": t.TempDir()})
	pedir(t, "POST", srv.URL+"/v1/projetos/5/tarefas", map[string]string{"titulo": "De outro perfil"})
	abrir := srv.URL + "/v1/perfis/1/abrir"

	// Só a abelha ligada: ninguém para atender, e ela abre o app.
	abelha := ouvirAbelha(t, srv.URL, "1")
	if status, r := pedir(t, "POST", abrir, map[string]any{"tarefa_id": 1}); status != http.StatusOK || r["janelas"] != 0.0 {
		t.Fatalf("sem janela: %d %v", status, r)
	}

	// Com a janela aberta, ela recebe o pedido (e a abelha também, que ignora).
	janela := ouvir(t, srv.URL, "1")
	if status, r := pedir(t, "POST", abrir, map[string]any{"tarefa_id": 2}); status != http.StatusOK || r["janelas"] != 1.0 {
		t.Fatalf("com a janela: %d %v", status, r)
	}
	if m := janela.esperar("tela.abrir", func(m map[string]any) bool { return m["tipo"] == "tela.abrir" }); m["tarefa_id"] != 2.0 {
		t.Errorf("pedido de abrir: %v", m)
	}
	if m := abelha.esperar("tela.abrir", func(m map[string]any) bool { return m["tipo"] == "tela.abrir" }); m["tarefa_id"] != 2.0 {
		t.Errorf("a abelha também recebe: %v", m)
	}
	// Só a janela: tarefa 0.
	if status, r := pedir(t, "POST", abrir, map[string]any{}); status != http.StatusOK || r["janelas"] != 1.0 {
		t.Errorf("só a janela: %d %v", status, r)
	}

	// A tarefa de outro perfil, que não existe ou um agente sem tarefa: recusados.
	for _, caso := range []struct {
		corpo  map[string]any
		status int
	}{
		{map[string]any{"tarefa_id": 5}, http.StatusNotFound},
		{map[string]any{"tarefa_id": 99}, http.StatusNotFound},
		{map[string]any{"tarefa_id": 1, "agente_id": 99}, http.StatusNotFound},
		{map[string]any{"agente_id": 1}, http.StatusBadRequest},
		{map[string]any{"tarefa_id": "1"}, http.StatusBadRequest},
		{map[string]any{"tarefa_id": 1, "outro": true}, http.StatusBadRequest},
	} {
		if status, _ := pedir(t, "POST", abrir, caso.corpo); status != caso.status {
			t.Errorf("%v: %d, esperado %d", caso.corpo, status, caso.status)
		}
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/perfis/99/abrir", map[string]any{}); status != http.StatusNotFound {
		t.Errorf("perfil que não existe: %d", status)
	}
}
