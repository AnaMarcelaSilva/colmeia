//go:build unix

package api

import (
	"context"
	"io"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

// servidorComBanco é o servidorComDados com o banco à mão, para gravar
// eventos que só os agentes gravam (o fim de uma sessão, com o tempo).
func servidorComBanco(t *testing.T) (*httptest.Server, *dados.Banco) {
	t.Helper()
	dir := t.TempDir()
	banco, err := dados.Abrir(filepath.Join(dir, "dados"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { banco.Fechar() })
	servidor := &Servidor{Bytes: new(atomic.Int64), Banco: banco, DirDados: dir, Versao: "teste", Tempos: temposDeTeste}
	srv := httptest.NewServer(servidor.Rotas())
	t.Cleanup(func() { servidor.Agentes.FecharTodos(); servidor.Encerrar() })
	t.Cleanup(srv.Close)
	return srv, banco
}

func lerTexto(t *testing.T, url string) (int, string) {
	t.Helper()
	r, err := http.Get(url)
	if err != nil {
		t.Fatal(err)
	}
	defer r.Body.Close()
	corpo, _ := io.ReadAll(r.Body)
	return r.StatusCode, string(corpo)
}

// Dois workspaces: estudos (loja-web e cliente-x) e trabalho-x (pedidos-api
// e outro loja-web), uma tarefa em cada projeto e uma sessão de agente com
// tempo em loja-web de estudos.
func doisWorkspaces(t *testing.T) (*httptest.Server, *dados.Banco) {
	t.Helper()
	srv, banco := servidorComBanco(t)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Pessoal"})
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Outro"})
	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "estudos"})
	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "trabalho-x"})
	pedir(t, "POST", srv.URL+"/v1/perfis/2/workspaces", map[string]string{"nome": "alheio"})
	for _, p := range []struct {
		ws   int
		nome string
	}{{1, "loja-web"}, {1, "cliente-x"}, {2, "pedidos-api"}, {2, "loja-web"}} {
		if status, r := pedir(t, "POST", srv.URL+"/v1/workspaces/"+itoa(p.ws)+"/projetos", map[string]string{"nome": p.nome, "caminho": t.TempDir()}); status != http.StatusOK {
			t.Fatalf("projeto %s: %d %v", p.nome, status, r)
		}
	}
	for projeto, titulo := range map[int]string{1: "Corrigir frete", 2: "Importar planilha", 3: "Fila de pedidos", 4: "Tela de cupons"} {
		pedir(t, "POST", srv.URL+"/v1/projetos/"+itoa(projeto)+"/tarefas", map[string]string{"titulo": titulo})
	}
	// As tarefas criadas hoje ainda não são trabalho; uma vai para a revisão.
	for tarefa := 1; tarefa <= 4; tarefa++ {
		pedir(t, "PATCH", srv.URL+"/v1/tarefas/"+itoa(tarefa), map[string]string{"coluna": "revisao"})
	}
	ctx := context.Background()
	tarefa, _, _, err := banco.Tarefa(ctx, 1)
	if err != nil {
		t.Fatal(err)
	}
	err = banco.Registrar(ctx, "agente.terminou", dados.Escopo{Perfil: 1, Projeto: tarefa.ProjetoID, Tarefa: 1, Agente: 7},
		map[string]any{"ferramenta": "claude", "papel": "dev", "titulo": "Corrigir frete", "motivo": "terminou", "trabalhando_s": 3900, "aguardando_s": 720})
	if err != nil {
		t.Fatal(err)
	}
	return srv, banco
}

func TestDailyESprintPorWorkspace(t *testing.T) {
	srv, _ := doisWorkspaces(t)
	base := srv.URL + "/v1/perfis/1/"

	// O workspace estudos: os dois projetos dele, nenhum do trabalho-x.
	status, deck := pedir(t, "GET", base+"apresentacao?tipo=daily&workspace=1", nil)
	if status != http.StatusOK {
		t.Fatalf("deck do workspace: %d %v", status, deck)
	}
	var secoes []string
	for _, s := range deck["slides"].([]any) {
		secoes = append(secoes, s.(map[string]any)["secao"].(string))
	}
	if got := strings.Join(secoes, ","); got != "cliente-x,loja-web" {
		t.Errorf("seções do workspace estudos: %s", got)
	}
	if p, _ := deck["periodo"].(string); !strings.HasSuffix(p, " · estudos · 2 projetos") {
		t.Errorf("período: %q", p)
	}

	// O perfil inteiro: os quatro projetos, os dois loja-web separados pelo workspace.
	_, deck = pedir(t, "GET", base+"apresentacao?tipo=sprint&ultimos=7", nil)
	secoes = nil
	for _, s := range deck["slides"].([]any) {
		secoes = append(secoes, s.(map[string]any)["secao"].(string))
	}
	if got := strings.Join(secoes, ","); got != "estudos · cliente-x,estudos · loja-web,trabalho-x · loja-web,trabalho-x · pedidos-api" {
		t.Errorf("seções do perfil: %s", got)
	}
	status, texto := lerTexto(t, base+"resumo?tipo=daily&formato=markdown")
	if status != http.StatusOK || !strings.Contains(texto, "trabalho-x · loja-web\n") || !strings.Contains(texto, "estudos · cliente-x\n") {
		t.Errorf("texto da daily do perfil: %d %s", status, texto)
	}
	if status, md := lerTexto(t, base+"resumo?tipo=sprint&ultimos=7&formato=markdown&workspace=2"); status != http.StatusOK ||
		!strings.Contains(md, "## loja-web") || !strings.Contains(md, "## pedidos-api") || strings.Contains(md, "cliente-x") {
		t.Errorf("sprint do trabalho-x: %d %s", status, md)
	}
	status, pagina := pedir(t, "GET", base+"linha-do-tempo?workspace=2", nil)
	if status != http.StatusOK {
		t.Fatalf("linha do tempo do workspace: %d %v", status, pagina)
	}
	for _, d := range pagina["dias"].([]any) {
		for _, item := range d.(map[string]any)["itens"].([]any) {
			if p := item.(map[string]any)["projeto"]; p != "loja-web" && p != "pedidos-api" {
				t.Errorf("item de fora do workspace: %v", item)
			}
		}
	}

	// Projeto e workspace juntos, workspace de outro perfil, ids ruins.
	for consulta, esperado := range map[string]int{
		"resumo?tipo=daily&projeto=1&workspace=1": http.StatusBadRequest,
		"resumo?tipo=daily&workspace=3":           http.StatusNotFound,
		"resumo?tipo=daily&workspace=99":          http.StatusNotFound,
		"apresentacao?tipo=daily&workspace=0":     http.StatusBadRequest,
		"linha-do-tempo?workspace=abc":            http.StatusBadRequest,
		"apresentacao?tipo=daily&projeto=4":       http.StatusOK,
	} {
		if status, _ := lerTexto(t, base+consulta); status != esperado {
			t.Errorf("%s: %d, esperado %d", consulta, status, esperado)
		}
	}
}

func TestTempoDosAgentesSoQuandoPedido(t *testing.T) {
	srv, banco := doisWorkspaces(t)
	base := srv.URL + "/v1/perfis/1/"
	rotas := []string{"resumo?tipo=daily", "resumo?tipo=sprint&ultimos=7", "apresentacao?tipo=daily", "apresentacao?tipo=sprint&ultimos=7", "linha-do-tempo",
		"resumo?tipo=sprint&ultimos=7&formato=markdown"}
	conferir := func(mostrar bool) {
		t.Helper()
		for _, rota := range rotas {
			status, corpo := lerTexto(t, base+rota)
			if status != http.StatusOK {
				t.Fatalf("%s: %d %s", rota, status, corpo)
			}
			tem := strings.Contains(corpo, "1h05") || strings.Contains(corpo, `"tempo_s"`) || strings.Contains(corpo, "esperou")
			if tem != mostrar {
				t.Errorf("%s com a opção %v: %s", rota, mostrar, corpo)
			}
		}
	}
	// Desligado por padrão: nada de tempo, mas a sessão aparece.
	status, perfis := pedirLista(t, srv.URL+"/v1/perfis")
	if status != http.StatusOK || perfis[0]["tempo_agentes"] != false {
		t.Fatalf("perfis: %v", perfis)
	}
	conferir(false)
	if _, corpo := lerTexto(t, base+"linha-do-tempo"); !strings.Contains(corpo, "terminou uma sessão") {
		t.Errorf("a sessão sumiu da linha do tempo: %s", corpo)
	}
	if status, _ := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"tempo_agentes": "sim"}); status != http.StatusBadRequest {
		t.Errorf("valor que não é booleano: %d", status)
	}
	if status, _ := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"tempo_agente": true}); status != http.StatusBadRequest {
		t.Errorf("campo desconhecido: %d", status)
	}
	if status, r := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"tempo_agentes": true}); status != http.StatusOK {
		t.Fatalf("ligar: %d %v", status, r)
	}
	conferir(true)
	// Vale só para este perfil, e fica gravado.
	_, perfis = pedirLista(t, srv.URL+"/v1/perfis")
	for _, p := range perfis {
		if (p["nome"] == "Pessoal") != (p["tempo_agentes"] == true) {
			t.Errorf("perfil %v", p)
		}
	}
	if status, _ := pedir(t, "PATCH", srv.URL+"/v1/perfis/99", map[string]any{"tempo_agentes": true}); status != http.StatusNotFound {
		t.Errorf("perfil que não existe: %d", status)
	}
	pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"tempo_agentes": false})
	conferir(false)
	// Os dados continuam na corrente de eventos (só a montagem esconde).
	eventos, err := banco.ListarEventos(context.Background(), dados.FiltroEventos{Perfil: 1, Tipos: []string{"agente.terminou", "perfil.tempo_agentes"}})
	if err != nil || len(eventos) != 3 || !strings.Contains(string(eventos[2].Dados), `"trabalhando_s":3900`) {
		t.Errorf("o tempo sumiu do histórico: %v %+v", err, eventos)
	}
}

// A troca da opção chega às telas do perfil só com o booleano.
func TestMensagemDaOpcaoDoTempo(t *testing.T) {
	s := &Servidor{}
	m := s.mensagemDoEvento(dados.Evento{Tipo: "perfil.tempo_agentes", Escopo: dados.Escopo{Perfil: 3}, Dados: []byte(`{"perfil":3,"mostrar":true}`)})
	if len(m) != 3 || m["tipo"] != "perfil.tempo_agentes" || m["perfil_id"] != int64(3) || m["mostrar"] != true {
		t.Errorf("mensagem: %v", m)
	}
}
