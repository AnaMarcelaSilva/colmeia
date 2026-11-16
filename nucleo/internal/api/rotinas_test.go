//go:build unix

package api

import (
	"fmt"
	"net/http"
	"strings"
	"testing"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

func TestRotinasDoWorkspace(t *testing.T) {
	srv := servidorComDados(t)
	perfilComAgente(t, srv.URL)
	rotinas := srv.URL + "/v1/workspaces/1/rotinas"

	// Uma rotina precisa de nome, pedido e pelo menos uma fonte.
	if status, r := pedir(t, "POST", rotinas, map[string]any{"nome": "Changelog", "pedido": "", "do_quadro": true}); status != http.StatusBadRequest {
		t.Fatalf("sem pedido: %d %v", status, r)
	}
	if status, r := pedir(t, "POST", rotinas, map[string]any{"nome": "Changelog", "pedido": "liste o que entrou"}); status != http.StatusBadRequest || !strings.Contains(fmt.Sprint(r["erro"]), "quadro") {
		t.Fatalf("sem fonte: %d %v", status, r)
	}
	if status, r := pedir(t, "POST", rotinas, map[string]any{"nome": "Changelog", "pedido": "liste", "do_quadro": true, "agenda_dias": []int{5}, "agenda_hora": "17h"}); status != http.StatusBadRequest {
		t.Fatalf("hora inválida: %d %v", status, r)
	}
	status, r := pedir(t, "POST", rotinas, map[string]any{"nome": " Changelog da semana ", "pedido": "Liste o que entrou na semana, por projeto.", "do_quadro": true, "do_codigo": true, "dias": 7, "agenda_dias": []int{5, 5}, "agenda_hora": "17:00", "ativa": true})
	if status != http.StatusOK {
		t.Fatalf("criar: %d %v", status, r)
	}
	if r["nome"] != "Changelog da semana" || r["agenda_hora"] != "17:00" || len(r["agenda_dias"].([]any)) != 1 {
		t.Fatalf("rotina criada: %v", r)
	}
	if r["proxima"] == nil || r["proxima_texto"] == nil {
		t.Fatalf("sem próxima rodada: %v", r)
	}
	id := int64(r["id"].(float64))

	// A lista traz a rotina e diz se o Claude Code está instalado.
	_, lista := pedir(t, "GET", rotinas, nil)
	if n := len(lista["rotinas"].([]any)); n != 1 {
		t.Fatalf("lista: %v", lista)
	}

	// Editar troca tudo; sem agenda, não há próxima.
	status, r = pedir(t, "PATCH", fmt.Sprintf("%s/v1/rotinas/%d", srv.URL, id), map[string]any{"nome": "Notas de versão", "pedido": "Texto para o cliente.", "do_quadro": true, "dias": 0, "ativa": true})
	if status != http.StatusOK || r["nome"] != "Notas de versão" || r["proxima"] != nil {
		t.Fatalf("editar: %d %v", status, r)
	}

	// Sem o Claude Code instalado, rodar explica.
	t.Setenv("PATH", t.TempDir()+":/usr/bin:/bin")
	if status, r := pedir(t, "POST", fmt.Sprintf("%s/v1/rotinas/%d/rodar", srv.URL, id), nil); status != http.StatusBadRequest || !strings.Contains(fmt.Sprint(r["erro"]), "não está instalado") {
		t.Fatalf("sem Claude Code: %d %v", status, r)
	}

	// Com um Claude Code que responde, a rodada começa na hora e termina pelo evento.
	claudeQueResponde(t, "```markdown\n### cliente-x\n- Analisar relatório: a tarefa foi criada.\n```")
	eventos := ouvirComo(t, "", srv.URL, "1")
	status, rodada := pedir(t, "POST", fmt.Sprintf("%s/v1/rotinas/%d/rodar", srv.URL, id), nil)
	if status != http.StatusOK || rodada["estado"] != "rodando" || rodada["origem"] != "voce" {
		t.Fatalf("rodar: %d %v", status, rodada)
	}
	fim := eventos.esperar("rodada terminou", func(e map[string]any) bool { return e["tipo"] == "rodada.terminou" })
	rd := fim["rodada"].(map[string]any)
	if rd["estado"] != "ok" || rd["erro"] != "" || !strings.Contains(fmt.Sprint(rd["resumo"]), "1 item") {
		t.Fatalf("rodada terminada: %v", rd)
	}
	_, completa := pedir(t, "GET", fmt.Sprintf("%s/v1/rodadas/%v", srv.URL, rd["id"]), nil)
	if texto := fmt.Sprint(completa["texto"]); !strings.HasPrefix(texto, "### cliente-x") || strings.Contains(texto, "```") {
		t.Fatalf("texto da rodada: %q", texto)
	}
	_, rodadas := pedir(t, "GET", fmt.Sprintf("%s/v1/rotinas/%d/rodadas", srv.URL, id), nil)
	if n := len(rodadas["rodadas"].([]any)); n != 1 {
		t.Fatalf("rodadas: %v", rodadas)
	}
	// A lista mostra a última rodada sem o texto.
	_, lista = pedir(t, "GET", rotinas, nil)
	ultima := lista["rotinas"].([]any)[0].(map[string]any)["ultima"].(map[string]any)
	if ultima["estado"] != "ok" || ultima["texto"] != nil {
		t.Fatalf("última na lista: %v", ultima)
	}

	// Remover leva as rodadas junto.
	if status, _ := pedir(t, "DELETE", fmt.Sprintf("%s/v1/rotinas/%d", srv.URL, id), nil); status != http.StatusOK {
		t.Fatalf("remover: %d", status)
	}
	if status, _ := pedir(t, "GET", fmt.Sprintf("%s/v1/rodadas/%v", srv.URL, rd["id"]), nil); status != http.StatusNotFound {
		t.Fatalf("rodada depois de remover: %d", status)
	}
}

func TestAgendaDaRotina(t *testing.T) {
	// Quarta-feira, 21/10/2026, 10:00 no fuso local.
	agora := time.Date(2026, 10, 21, 10, 0, 0, 0, time.Local)
	r := dados.Rotina{Ativa: true, AgendaDias: []int{1, 3, 5}, AgendaHora: "17:00"}
	p, ok := proximaRodada(r, agora)
	if !ok || p.Weekday() != time.Wednesday || p.Hour() != 17 {
		t.Fatalf("próxima hoje: %v %v", p, ok)
	}
	if texto := textoDoMomento(p, agora); texto != "hoje às 17:00" {
		t.Fatalf("texto: %q", texto)
	}
	// Depois da hora, a próxima é sexta.
	p, _ = proximaRodada(r, agora.Add(8*time.Hour))
	if p.Weekday() != time.Friday {
		t.Fatalf("próxima depois da hora: %v", p)
	}
	if texto := textoDoMomento(p, agora.Add(8*time.Hour)); texto != "sexta, 23/10 às 17:00" {
		t.Fatalf("texto sexta: %q", texto)
	}
	if texto := textoDoMomento(agora.AddDate(0, 0, 1), agora); !strings.HasPrefix(texto, "amanhã às") {
		t.Fatalf("texto amanhã: %q", texto)
	}
	// Inativa ou sem dias: nada.
	if _, ok := proximaRodada(dados.Rotina{Ativa: false, AgendaDias: []int{1}, AgendaHora: "17:00"}, agora); ok {
		t.Fatal("inativa tem próxima")
	}
	// Vencida: só depois da hora, e só nos dias da agenda.
	if _, ok := horarioVencido(r, agora); ok {
		t.Fatal("venceu antes da hora")
	}
	if v, ok := horarioVencido(r, agora.Add(8*time.Hour)); !ok || v.Hour() != 17 {
		t.Fatalf("vencida: %v %v", v, ok)
	}
	if _, ok := horarioVencido(r, agora.AddDate(0, 0, 1).Add(8*time.Hour)); ok {
		t.Fatal("venceu num dia fora da agenda")
	}
}

func TestAgendadorRodaUmaVezPorHorario(t *testing.T) {
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	claudeQueResponde(t, "### W\n- Nada de novo.")
	rotinas := srv.URL + "/v1/workspaces/1/rotinas"
	agora := time.Now().In(time.Local)
	semana := int(agora.Weekday())
	if semana == 0 {
		semana = 7
	}
	// Agendada para um minuto atrás, hoje: já venceu.
	hora := agora.Add(-time.Minute).Format("15:04")
	status, r := pedirComo(t, tokenTela, "POST", rotinas, map[string]any{"nome": "Fala da daily", "pedido": "Três frases.", "do_quadro": true, "agenda_dias": []int{semana}, "agenda_hora": hora, "ativa": true})
	if status != http.StatusOK {
		t.Fatalf("criar: %d %v", status, r)
	}
	id := int64(r["id"].(float64))
	// O horário venceu antes de a rotina existir: não roda.
	servidor.dispararVencidas(time.Now())
	time.Sleep(100 * time.Millisecond)
	if _, err := servidor.Banco.UltimaRodada(t.Context(), id, dados.OrigemAgenda); err == nil {
		t.Fatal("rodou um horário anterior à criação")
	}
	// Um horário depois da criação, já vencido: roda.
	servidor.dispararVencidas(time.Now().Add(24 * time.Hour * 7))
	for i := 0; i < 100; i++ {
		if u, err := servidor.Banco.UltimaRodada(t.Context(), id, dados.OrigemAgenda); err == nil && u.Estado != dados.RodadaRodando {
			break
		}
		time.Sleep(50 * time.Millisecond)
	}
	u, err := servidor.Banco.UltimaRodada(t.Context(), id, dados.OrigemAgenda)
	if err != nil || u.Estado != dados.RodadaOk {
		t.Fatalf("rodada da agenda: %v %v", u, err)
	}
	// O mesmo horário não roda de novo.
	servidor.dispararVencidas(time.Now())
	time.Sleep(100 * time.Millisecond)
	if rodadas, _ := servidor.Banco.ListarRodadas(t.Context(), id, 0); len(rodadas) != 1 {
		t.Fatalf("rodou de novo: %v", rodadas)
	}
}

func TestTextoEEntradaDoAgente(t *testing.T) {
	srv, _ := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	status, a := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]any{"ferramenta": "shell", "papel": "dev", "cols": 80, "rows": 24})
	if status != http.StatusOK {
		t.Fatalf("criar agente: %d %v", status, a)
	}
	id := int64(a["id"].(float64))
	// Escrever um comando com enter e ler o que o shell mostrou.
	if status, r := pedirComo(t, tokenTela, "POST", fmt.Sprintf("%s/v1/agentes/%d/entrada", srv.URL, id), map[string]any{"texto": "echo colmeia-abelha-le-texto", "enter": true}); status != http.StatusOK {
		t.Fatalf("entrada: %d %v", status, r)
	}
	var linhas []any
	for i := 0; i < 100; i++ {
		_, r := pedirComo(t, tokenTela, "GET", fmt.Sprintf("%s/v1/agentes/%d/texto?linhas=10", srv.URL, id), nil)
		linhas, _ = r["linhas"].([]any)
		if r["rodando"] == true && strings.Contains(fmt.Sprint(linhas), "colmeia-abelha-le-texto") && len(linhas) >= 2 {
			break
		}
		time.Sleep(50 * time.Millisecond)
	}
	if !strings.Contains(fmt.Sprint(linhas), "colmeia-abelha-le-texto") {
		t.Fatalf("texto do agente: %v", linhas)
	}
	// Caracteres de controle não entram.
	if status, _ := pedirComo(t, tokenTela, "POST", fmt.Sprintf("%s/v1/agentes/%d/entrada", srv.URL, id), map[string]any{"texto": "a\x1bb", "enter": true}); status != http.StatusBadRequest {
		t.Fatalf("controle: %d", status)
	}
	// Um agente que não existe não está rodando.
	if _, r := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/agentes/999/texto", nil); r["rodando"] != false {
		t.Fatalf("agente inexistente: %v", r)
	}
}
