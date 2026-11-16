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

// O agente propõe um Play (no projeto da tarefa ou em outro do workspace),
// que não roda até a tela aceitar; a tela ouve a proposta e a aceitação, e
// a lista por workspace traz os dois projetos.
func TestPlayPropostoPeloAgente(t *testing.T) {
	claudeQueAnota(t)
	srv, servidor := servidorComAgentes(t)
	pasta := montarTarefas(t, srv.URL)
	os.MkdirAll(filepath.Join(pasta, "app"), 0o755)
	outra := t.TempDir()
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "loja-api", "caminho": outra})
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	dir := filepath.Join(servidor.DirCanal, "agentes")
	esperarAte(t, "o token do agente", func() bool { return len(lerArquivo(filepath.Join(dir, "1.token"))) == 64 })
	token := lerArquivo(filepath.Join(dir, "1.token"))
	tela := ouvirComo(t, tokenTela, srv.URL, "1")
	tela.proxima()

	// Sem nada ainda.
	status, r := pedirComo(t, token, "GET", srv.URL+"/v1/agente/comandos", nil)
	if status != http.StatusOK || len(r["comandos"].([]any)) != 0 || r["projeto"] != "loja-web" {
		t.Fatalf("lista vazia: %d %v", status, r)
	}
	// Propõe no projeto da tarefa e em outro do workspace; um projeto de fora
	// e um segredo são recusados.
	status, r = pedirComo(t, token, "POST", srv.URL+"/v1/agente/comandos", map[string]any{
		"nome": "app", "comando": "echo app", "pasta": "app", "ambiente": []map[string]string{{"nome": "DNS", "valor": ""}}})
	if status != http.StatusOK || r["proposta"] != true || r["projeto"] != "loja-web" {
		t.Fatalf("propor: %d %v", status, r)
	}
	aviso := tela.esperar("proposta", tipo("comando.proposto"))
	if aviso["nome"] != "app" || aviso["quem"] != "Claude Code (dev)" || aviso["projeto_id"] != float64(1) || aviso["tarefa_id"] != float64(1) {
		t.Errorf("aviso da proposta: %v", aviso)
	}
	if status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/comandos", map[string]any{"nome": "api", "comando": "echo api", "projeto": "Loja-API"}); status != http.StatusOK || r["projeto"] != "loja-api" {
		t.Errorf("propor em outro projeto: %d %v", status, r)
	}
	if status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/comandos", map[string]any{"nome": "x", "comando": "ls", "projeto": "outro"}); status != http.StatusBadRequest || !strings.Contains(r["erro"].(string), "loja-api") {
		t.Errorf("projeto de fora: %d %v", status, r)
	}
	if status, _ := pedirComo(t, token, "POST", srv.URL+"/v1/agente/comandos", map[string]any{"nome": "x", "comando": "ls", "ambiente": []map[string]string{{"nome": "TOKEN", "valor": "ghp_abcdefghijklmnopqrstuvwxyz0123456789"}}}); status != http.StatusBadRequest {
		t.Errorf("segredo: %d", status)
	}
	if status, _ := pedirComo(t, token, "POST", srv.URL+"/v1/agente/comandos", map[string]any{"nome": "app", "comando": "ls"}); status != http.StatusBadRequest {
		t.Errorf("nome repetido: %d", status)
	}
	// A proposta aparece na lista do projeto e na do workspace, sem rodar.
	status, lista := pedirListaComo(t, tokenTela, srv.URL+"/v1/workspaces/1/comandos")
	if status != http.StatusOK || len(lista) != 2 || lista[0]["projeto_nome"] != "loja-api" || lista[1]["nome"] != "app" || lista[1]["proposta"] != true || lista[1]["proposta_por"] != "Claude Code (dev)" {
		t.Fatalf("lista do workspace: %d %v", status, lista)
	}
	id := int(lista[1]["id"].(float64))
	if status, r := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/comandos/"+itoa(id)+"/rodar", nil); status != http.StatusBadRequest || !strings.Contains(r["erro"].(string), "aceite") {
		t.Errorf("rodar a proposta: %d %v", status, r)
	}
	// A tela não cria com origem agente; o agente lista as duas propostas sem os valores.
	if status, _ := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/projetos/1/comandos", map[string]any{"nome": "y", "comando": "ls", "origem": "agente"}); status != http.StatusBadRequest {
		t.Errorf("origem agente pela tela: %d", status)
	}
	status, r = pedirComo(t, token, "GET", srv.URL+"/v1/agente/comandos", nil)
	comandos := r["comandos"].([]any)
	if status != http.StatusOK || len(comandos) != 2 || comandos[1].(map[string]any)["proposta"] != true || comandos[1].(map[string]any)["ambiente"] != nil {
		t.Errorf("lista do agente: %d %v", status, r)
	}
	// Aceita: vira uma configuração comum, roda, e a tela ouve.
	if status, r := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/comandos/"+itoa(id)+"/aceitar", nil); status != http.StatusOK || r["proposta"] != false || r["origem"] != "agente" {
		t.Fatalf("aceitar: %d %v", status, r)
	}
	if aviso := tela.esperar("aceito", tipo("comando.aceito")); aviso["comando_id"] != float64(id) || aviso["quem"] != "Claude Code (dev)" {
		t.Errorf("aviso do aceite: %v", aviso)
	}
	if status, _ := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/comandos/"+itoa(id)+"/aceitar", nil); status != http.StatusBadRequest {
		t.Errorf("aceitar de novo: %d", status)
	}
	if status, r := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/comandos/"+itoa(id)+"/rodar", nil); status != http.StatusOK || r["rodando"] != true {
		t.Errorf("rodar depois de aceitar: %d %v", status, r)
	}
	tela.esperar("fim", tipo("comando.terminou"))
	// A linha do tempo conta a proposta e o aceite.
	status, linha := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/linha-do-tempo", nil)
	bruto, _ := json.Marshal(linha)
	if status != http.StatusOK || !strings.Contains(string(bruto), "propôs o Play “app” em loja-web") || !strings.Contains(string(bruto), "aceitou o Play “app”") {
		t.Errorf("linha do tempo: %d %s", status, bruto)
	}
}

// pedirListaComo faz um GET com o token da tela que responde uma lista.
func pedirListaComo(t *testing.T, token, url string) (int, []map[string]any) {
	t.Helper()
	r, _ := http.NewRequest("GET", url, nil)
	r.Header.Set("Authorization", "Bearer "+token)
	resposta, err := http.DefaultClient.Do(r)
	if err != nil {
		t.Fatal(err)
	}
	defer resposta.Body.Close()
	var lista []map[string]any
	json.NewDecoder(resposta.Body).Decode(&lista)
	return resposta.StatusCode, lista
}
