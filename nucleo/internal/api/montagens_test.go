//go:build unix

package api

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"testing"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/formato"
)

func TestMontagemSegueATarefaEDesenhaALousaDaReuniao(t *testing.T) {
	esperaSimples, esperaMontador := EsperaSimples, EsperaMontador
	EsperaSimples, EsperaMontador = 10*time.Millisecond, time.Hour
	t.Cleanup(func() { EsperaSimples, EsperaMontador = esperaSimples, esperaMontador })
	claudeQueAnota(t)
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)

	// A tarefa entra em andamento com a nota do dia: a montagem simples sai da nota.
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/1", map[string]string{"coluna": "trabalhando"})
	pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/tarefas/1/notas", map[string]string{"tipo": "daily", "periodo": hoje(), "texto": formato.ExemploNotaDaily})
	var m map[string]any
	esperarAte(t, "a montagem da nota", func() bool {
		_, m = pedirComo(t, tokenTela, "GET", srv.URL+"/v1/tarefas/1/montagem", nil)
		cartoes, _ := m["cartoes"].([]any)
		return m["origem"] == "simples" && len(cartoes) == 4
	})
	if m["topico"] != "Nova tela de pedidos" || !strings.HasPrefix(fmt.Sprint(m["frase"]), "A gestão web de cada loja") {
		t.Fatalf("montagem: %v", m)
	}
	cartoes := m["cartoes"].([]any)
	if cartoes[2].(map[string]any)["titulo"] != "Como funciona" || cartoes[3].(map[string]any)["titulo"] != "Falta" {
		t.Errorf("cartões: %v", cartoes)
	}

	// O deck leva a montagem de cada tarefa.
	_, deck := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/apresentacao?tipo=daily", nil)
	slides, _ := deck["slides"].([]any)
	if len(slides) != 1 || slides[0].(map[string]any)["montagem"].(map[string]any)["topico"] != "Nova tela de pedidos" {
		t.Fatalf("deck: %v", deck)
	}

	// A lousa da daily de hoje nasce desenhada: o cartão da tarefa, os
	// cartões em coluna e as setas, tudo do montador e marcado com a tarefa.
	status, lousa := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/perfis/1/daily/lousa", nil)
	if status != http.StatusOK || lousa["lousa"].(map[string]any)["dono"].(map[string]any)["dia"] != hoje() {
		t.Fatalf("lousa da daily: %d %v", status, lousa)
	}
	contar := func(elementos []any) (tarefas, notas, ligacoes, montador int, cartaoDaTarefa map[string]any) {
		for _, e := range elementos {
			el := e.(map[string]any)
			switch el["tipo"] {
			case "tarefa":
				tarefas++
				cartaoDaTarefa = el
			case "nota":
				notas++
			case "ligacao":
				ligacoes++
			}
			if el["autor"] == "montador" {
				montador++
			}
		}
		return
	}
	elementos := lousa["elementos"].([]any)
	tarefas, notas, ligacoes, montador, cartao := contar(elementos)
	if tarefas != 1 || notas != 4 || ligacoes != 4 || montador != len(elementos) || cartao["montagem_de"] != 1.0 || cartao["tarefa_ref"] != 1.0 {
		t.Fatalf("desenho: %d tarefas, %d notas, %d ligações, %d do montador: %v", tarefas, notas, ligacoes, montador, elementos)
	}
	lousaID := int64(lousa["lousa"].(map[string]any)["id"].(float64))

	// Mover um cartão o faz seu: remontar não mexe nele, e refaz o resto.
	var nota map[string]any
	for _, e := range elementos {
		if el := e.(map[string]any); el["tipo"] == "nota" && el["titulo"] == "Falta" {
			nota = el
		}
	}
	status, r := pedirComo(t, tokenTela, "POST", fmt.Sprintf("%s/v1/lousas/%d/operacoes", srv.URL, lousaID), map[string]any{"operacoes": []map[string]any{
		{"op": "alterar", "id": nota["id"], "versao": nota["versao"], "campos": map[string]any{"x": 2000}},
	}})
	if status != http.StatusOK || r["elementos"].([]any)[0].(map[string]any)["autor"] != "voce" {
		t.Fatalf("mover o cartão: %d %v", status, r)
	}
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/montagem", nil)
	_, lousa = pedirComo(t, tokenTela, "GET", fmt.Sprintf("%s/v1/lousas/%d", srv.URL, lousaID), nil)
	elementos = lousa["elementos"].([]any)
	seu := 0
	for _, e := range elementos {
		if el := e.(map[string]any); el["autor"] == "voce" {
			seu++
			if el["id"] != nota["id"] || el["x"] != 2000.0 {
				t.Errorf("o seu cartão mudou: %v", el)
			}
		}
	}
	tarefas, notas, _, montador, _ = contar(elementos)
	if seu != 1 || tarefas != 1 || notas != 5 || montador != len(elementos)-1 {
		t.Errorf("depois de remontar: %d seus, %d tarefas, %d notas, %d do montador", seu, tarefas, notas, montador)
	}

	// Outra tarefa entra no dia: a lousa ganha a coluna dela, ligada à anterior.
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/2", map[string]string{"coluna": "trabalhando"})
	esperarAte(t, "a segunda coluna", func() bool {
		_, lousa = pedirComo(t, tokenTela, "GET", fmt.Sprintf("%s/v1/lousas/%d", srv.URL, lousaID), nil)
		tarefas, _, _, _, _ = contar(lousa["elementos"].([]any))
		return tarefas == 2
	})
	// Na ordem do deck (a mais recente antes), lado a lado, e o último
	// cartão da primeira coluna liga ao cartão da tarefa seguinte.
	var primeira, segunda map[string]any
	for _, e := range lousa["elementos"].([]any) {
		if el := e.(map[string]any); el["tipo"] == "tarefa" {
			if primeira == nil || el["x"].(float64) < primeira["x"].(float64) {
				primeira, segunda = el, primeira
			} else {
				segunda = el
			}
		}
	}
	if segunda["x"].(float64) <= primeira["x"].(float64) || segunda["y"] != primeira["y"] {
		t.Errorf("colunas: %v %v", primeira, segunda)
	}
	var ultimo map[string]any
	for _, e := range lousa["elementos"].([]any) {
		if el := e.(map[string]any); el["tipo"] == "nota" && el["montagem_de"] == primeira["montagem_de"] && el["autor"] == "montador" && (ultimo == nil || el["y"].(float64) > ultimo["y"].(float64)) {
			ultimo = el
		}
	}
	ligada := false
	for _, e := range lousa["elementos"].([]any) {
		if el := e.(map[string]any); el["tipo"] == "ligacao" && el["de"] == ultimo["id"] && el["para"] == segunda["id"] {
			ligada = true
		}
	}
	if !ligada {
		t.Error("o último cartão da primeira coluna não liga à tarefa seguinte")
	}

	// A lousa da sprint atual também nasce desenhada, com as duas tarefas.
	_, sprints := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/sprints", nil)
	status, lousa = pedirComo(t, tokenTela, "POST", fmt.Sprintf("%s/v1/sprints/%v/lousa", srv.URL, sprints["atual"]), nil)
	tarefas, _, _, _, _ = contar(lousa["elementos"].([]any))
	if status != http.StatusOK || tarefas != 2 || lousa["lousa"].(map[string]any)["dono"].(map[string]any)["sprint_id"] != sprints["atual"] {
		t.Errorf("lousa da sprint: %d %d tarefas %v", status, tarefas, lousa["lousa"])
	}

	// O agente pede a montagem da tarefa dele; a tela, não por essa rota.
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	token := lerArquivo(servidor.DirCanal + "/agentes/1.token")
	if status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/montagem", nil); status != http.StatusOK || r["montando"] != true {
		t.Errorf("agente: %d %v", status, r)
	}
	if status, _ := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/agente/montagem", nil); status != http.StatusUnauthorized {
		t.Errorf("tela na rota do agente: %d", status)
	}

	// O montador com IA: ligado por padrão no perfil, desliga e liga pela
	// opção; com o Claude Code respondendo, a montagem vira dele.
	perfil := func() bool {
		p, err := servidor.Banco.Perfil(t.Context(), 1)
		if err != nil {
			t.Fatal(err)
		}
		return p.Montador
	}
	if !perfil() {
		t.Error("o montador nasce ligado")
	}
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/perfis/1", map[string]bool{"montador": false})
	if perfil() {
		t.Error("desligar não desligou")
	}
	claudeQueResponde(t, `{"topico":"Endereço da loja","frase":"O painel abre a gestão de cada loja pelo endereço certo.","cartoes":[{"titulo":"Endereço da loja","texto":"Campo novo no instalador e endpoint novo na central."},{"titulo":"Falta","texto":"Decidir com o time onde fica a gestão web."}]}`)
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/montagem", map[string]bool{"com_agente": true})
	time.Sleep(200 * time.Millisecond)
	if _, m = pedirComo(t, tokenTela, "GET", srv.URL+"/v1/tarefas/1/montagem", nil); m["origem"] != "simples" {
		t.Errorf("desligado, o montador não roda: %v", m)
	}
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/perfis/1", map[string]bool{"montador": true})
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/montagem", map[string]bool{"com_agente": true})
	esperarAte(t, "a montagem com IA", func() bool {
		_, m = pedirComo(t, tokenTela, "GET", srv.URL+"/v1/tarefas/1/montagem", nil)
		return m["origem"] == "montador"
	})
	if m["topico"] != "Endereço da loja" || len(m["cartoes"].([]any)) != 2 {
		t.Errorf("montagem com IA: %v", m)
	}
	// A lousa acompanha: a coluna da tarefa 1 agora tem dois cartões.
	esperarAte(t, "a lousa com a montagem nova", func() bool {
		_, lousa = pedirComo(t, tokenTela, "GET", fmt.Sprintf("%s/v1/lousas/%d", srv.URL, lousaID), nil)
		n := 0
		for _, e := range lousa["elementos"].([]any) {
			if el := e.(map[string]any); el["tipo"] == "nota" && el["montagem_de"] == 1.0 && el["autor"] == "montador" {
				n++
			}
		}
		return n == 2
	})
}

// A montagem feita por falta dentro de um pedido (a página abrindo) vai na
// resposta e não avisa a tela: cada aviso faria a tela pedir a página de
// novo, e cada pedido montaria outra vez o que ainda não estava gravado.
func TestMontagemFeitaPorFaltaNaoAvisaATela(t *testing.T) {
	esperaSimples, esperaMontador := EsperaSimples, EsperaMontador
	EsperaSimples, EsperaMontador = time.Hour, time.Hour
	t.Cleanup(func() { EsperaSimples, EsperaMontador = esperaSimples, esperaMontador })
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/1", map[string]string{"coluna": "trabalhando"})
	tela, err := servidor.Avisos.Assinar(1)
	if err != nil {
		t.Fatal(err)
	}
	defer tela.Cancelar()

	_, deck := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/apresentacao?tipo=daily", nil)
	slides, _ := deck["slides"].([]any)
	if len(slides) != 1 || slides[0].(map[string]any)["montagem"].(map[string]any)["origem"] != "simples" {
		t.Fatalf("deck sem a montagem simples: %v", deck)
	}
	if _, m := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/tarefas/1/montagem", nil); m["origem"] != "simples" {
		t.Fatalf("a montagem não ficou guardada: %v", m)
	}
	ctx, cancelar := context.WithTimeout(context.Background(), 300*time.Millisecond)
	defer cancelar()
	for {
		bruto, err := tela.Proxima(ctx)
		if err != nil {
			break
		}
		var aviso map[string]any
		_ = json.Unmarshal(bruto, &aviso)
		if aviso["tipo"] == "montagem.mudou" {
			t.Fatalf("a montagem por falta avisou a tela: %s", bruto)
		}
	}
}
