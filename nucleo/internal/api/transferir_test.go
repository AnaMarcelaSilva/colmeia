//go:build unix

package api

import (
	"context"
	"fmt"
	"net/http"
	"os/exec"
	"strings"
	"testing"
)

// Um repositório git com a branch "feat/pedidos", para a branch seguir a tarefa.
func repoComBranch(t *testing.T, branch string) string {
	t.Helper()
	dir := t.TempDir()
	for _, args := range [][]string{{"init", "-q", "-b", "main"}, {"-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "início"}, {"branch", branch}} {
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		if saida, err := cmd.CombinedOutput(); err != nil {
			t.Skipf("git %v: %v %s", args, err, saida)
		}
	}
	return dir
}

func TestTarefaMudaDeProjetoSemPerderNada(t *testing.T) {
	claudeQueAnota(t)
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	eventos := ouvirComo(t, tokenTela, srv.URL, "1")
	// Um segundo projeto no mesmo perfil (git, com a branch da tarefa) e um
	// terceiro sem git; um projeto de outro perfil não serve de destino.
	repo := repoComBranch(t, "feat/pedidos")
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "erp", "caminho": repo})
	pasta := t.TempDir()
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "notas", "caminho": pasta})
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Outro"})
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/perfis/2/workspaces", map[string]string{"nome": "W2"})
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/workspaces/2/projetos", map[string]string{"nome": "alheio", "caminho": t.TempDir()})
	pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/tarefas/1/notas", map[string]string{"tipo": "daily", "periodo": "2026-10-02", "texto": "Tela pronta"})
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/1", map[string]string{"coluna": "trabalhando"})

	// Com agente, não vai; sem destino ou com o de outro perfil, também não.
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	if status, r := pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/tarefas/1/projeto", map[string]any{"projeto": 2}); status != http.StatusBadRequest || !strings.Contains(r["erro"].(string), "1 agente") {
		t.Errorf("com agente: %d %v", status, r)
	}
	pedirComo(t, tokenTela, "DELETE", srv.URL+"/v1/agentes/1", nil)
	for _, ruim := range []map[string]any{{"projeto": 4}, {"projeto": 99}, {"projeto": 1}} {
		if status, _ := pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/tarefas/1/projeto", ruim); status != http.StatusBadRequest {
			t.Errorf("aceitou %v: %d", ruim, status)
		}
	}

	// Para o git com a branch: ela segue, a coluna fica, a nota também.
	status, r := pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/tarefas/1/projeto", map[string]any{"projeto": 2})
	if status != http.StatusOK || r["projeto_id"] != 2.0 || r["coluna"] != "trabalhando" {
		t.Fatalf("transferir: %d %v", status, r)
	}
	m := eventos.esperar("tarefa transferida", func(m map[string]any) bool {
		tarefa, _ := m["tarefa"].(map[string]any)
		return m["tipo"] == "tarefa.atualizada" && tarefa["projeto_id"] == 2.0
	})
	if tarefa, _ := m["tarefa"].(map[string]any); tarefa["id"] != 1.0 {
		t.Errorf("evento: %v", m)
	}
	if _, n := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/tarefas/1/descricao", nil); n["texto"] != "Tela pronta" {
		t.Errorf("a nota não foi junto: %v", n)
	}
	// A branch só existe nesse repositório depois de escolhida.
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/1", map[string]string{"branch": "feat/pedidos"})
	status, r = pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/tarefas/1/projeto", map[string]any{"projeto": 3})
	if status != http.StatusOK || r["branch"] != "" {
		t.Errorf("para a pasta sem git a branch devia sumir: %d %v", status, r)
	}
	status, r = pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/tarefas/1/projeto", map[string]any{"projeto": 2})
	if status != http.StatusOK || r["branch"] != "" {
		t.Errorf("de volta ao git, sem branch: %d %v", status, r)
	}

	// A linha do tempo conta a mudança.
	_, linha := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/linha-do-tempo?tarefa=1", nil)
	texto := strings.Join(textos(linha), "\n")
	if !strings.Contains(texto, "Levou “Nova tela de pedidos” de loja-web para erp.") || !strings.Contains(texto, "de erp para notas.") {
		t.Errorf("linha do tempo: %s", texto)
	}
	_ = servidor
}

// textos junta os textos dos itens da linha do tempo, em qualquer dia.
func textos(linha map[string]any) []string {
	var lista []string
	dias, _ := linha["dias"].([]any)
	for _, d := range dias {
		itens, _ := d.(map[string]any)["itens"].([]any)
		for _, i := range itens {
			if s, ok := i.(map[string]any)["texto"].(string); ok {
				lista = append(lista, s)
			}
		}
	}
	return lista
}

func TestAgenteCriaETransfereNoProjetoPedido(t *testing.T) {
	claudeQueAnota(t)
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "Estoque Central", "caminho": t.TempDir()})
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	token := lerArquivo(servidor.DirCanal + "/agentes/1.token")

	// Ele vê os projetos do perfil, com o dele marcado.
	status, r := pedirComo(t, token, "GET", srv.URL+"/v1/agente/projetos", nil)
	projetos, _ := r["projetos"].([]any)
	// Na ordem da barra lateral (workspace e nome): Estoque Central antes de loja-web.
	if status != http.StatusOK || len(projetos) != 2 || projetos[0].(map[string]any)["nome"] != "Estoque Central" || projetos[1].(map[string]any)["seu"] != true {
		t.Fatalf("projetos: %d %v", status, r)
	}

	// Cria onde a usuária pediu, pelo nome, sem diferença de maiúsculas; um nome
	// errado volta com a lista.
	status, r = pedirComo(t, token, "POST", srv.URL+"/v1/agente/tarefas", map[string]any{"titulo": "Botão de reabrir", "projeto": "estoque central", "descricao": "Ao ler uma comanda finalizada"})
	if status != http.StatusOK || r["projeto"] != "Estoque Central" {
		t.Fatalf("criar no outro projeto: %d %v", status, r)
	}
	if tarefa, _, _, err := servidor.Banco.Tarefa(context.Background(), 3); err != nil || tarefa.ProjetoID != 2 || tarefa.Titulo != "Botão de reabrir" {
		t.Errorf("a tarefa não entrou no Estoque Central: %+v %v", tarefa, err)
	}
	if status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/tarefas", map[string]any{"titulo": "x", "projeto": "estoquecentral"}); status != http.StatusBadRequest || !strings.Contains(r["erro"].(string), "W/Estoque Central") {
		t.Errorf("nome errado: %d %v", status, r)
	}

	// Transfere outra tarefa do projeto dele; a própria, não (ele roda nela).
	for _, ruim := range []map[string]any{{"tarefa": 1, "projeto": "Estoque Central"}, {"tarefa": 3, "projeto": "loja-web"}, {"tarefa": 2, "projeto": "nada"}} {
		if status, _ := pedirComo(t, token, "POST", srv.URL+"/v1/agente/transferir", ruim); status != http.StatusBadRequest {
			t.Errorf("aceitou %v: %d", ruim, status)
		}
	}
	status, r = pedirComo(t, token, "POST", srv.URL+"/v1/agente/transferir", map[string]any{"tarefa": 2, "projeto": "W/Estoque Central"})
	if status != http.StatusOK || r["projeto"] != "Estoque Central" {
		t.Fatalf("transferir: %d %v", status, r)
	}
	if tarefa, _, _, err := servidor.Banco.Tarefa(context.Background(), 2); err != nil || tarefa.ProjetoID != 2 {
		t.Errorf("a tarefa 2 não foi: %+v %v", tarefa, err)
	}
	// Ela saiu da lista do projeto dele.
	if _, r := pedirComo(t, token, "GET", srv.URL+"/v1/agente/tarefas", nil); strings.Contains(fmt.Sprint(r), "Relatório do cliente-x") {
		t.Errorf("a tarefa 2 ainda aparece em loja-web: %v", r)
	}
	if status, _ := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/agente/transferir", map[string]any{"tarefa": 2, "projeto": "loja-web"}); status != http.StatusUnauthorized {
		t.Errorf("token da tela na rota do agente: %d", status)
	}
}
