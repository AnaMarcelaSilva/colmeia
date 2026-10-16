//go:build unix

package api

import (
	"context"
	"strings"
	"testing"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

func notaDeHoje(t *testing.T, s *Servidor, tarefa int64) (daily, sprint string) {
	t.Helper()
	ctx := context.Background()
	d, err := s.Banco.Nota(ctx, tarefa, "daily", hoje())
	if err != nil {
		t.Fatal(err)
	}
	atual, err := s.Banco.SprintAtual(ctx, 1, hoje())
	if err != nil {
		t.Fatal(err)
	}
	sp, err := s.Banco.Nota(ctx, tarefa, "sprint", atual.ChaveNota())
	if err != nil {
		t.Fatal(err)
	}
	return d.Texto, sp.Texto
}

func frase(escrevendo bool) func(map[string]any) bool {
	return func(m map[string]any) bool { return m["tipo"] == "tarefa.frase" && m["escrevendo"] == escrevendo }
}

func TestFraseMontadaSemAgenteENuncaPorCimaDoSeu(t *testing.T) {
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	// Você já escreveu na daily da tarefa 2: nada muda nela.
	pedirComo(t, tokenTela, "PUT", srv.URL+"/v1/tarefas/2/notas", map[string]string{"tipo": "daily", "periodo": hoje(), "texto": "Meu texto"})

	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/1", map[string]string{"coluna": "concluido"})
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/2", map[string]string{"coluna": "revisao"})
	esperarAte(t, "a frase montada", func() bool { d, s := notaDeHoje(t, servidor, 1); return d != "" && s != "" })
	if d, s := notaDeHoje(t, servidor, 1); d != "Concluí a tarefa “Nova tela de pedidos”." || s != d {
		t.Errorf("frase montada: %q / %q", d, s)
	}
	time.Sleep(200 * time.Millisecond)
	if d, s := notaDeHoje(t, servidor, 2); d != "Meu texto" || s != "" {
		t.Errorf("mexeu no que você escreveu: %q / %q", d, s)
	}
}

func TestFraseMontadaCitaAUltimaMensagem(t *testing.T) {
	tarefa := dados.Tarefa{Titulo: "Relatório do cliente-x", Coluna: "revisao"}
	if f := fraseMontada(tarefa, "  ajustar o total por mês\nmais detalhes"); f != "Deixei pronta para revisão a tarefa “Relatório do cliente-x”. O último pedido foi: “ajustar o total por mês”." {
		t.Errorf("frase: %q", f)
	}
	if f := fraseMontada(tarefa, strings.Repeat("a", 300)); !strings.HasSuffix(f, "…”.") || len([]rune(f)) > 230 {
		t.Errorf("citação longa: %q", f)
	}
}

func TestFrasePedidaAoAgente(t *testing.T) {
	_, linhas := claudeQueAnota(t)
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	eventos := ouvirComo(t, tokenTela, srv.URL, "1")
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	token := lerArquivo(servidor.DirCanal + "/agentes/1.token")

	// Revisão com o agente rodando: o pedido da frase entra no terminal dele.
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/1", map[string]string{"coluna": "revisao"})
	eventos.esperar("escrevendo a frase", frase(true))
	esperarAte(t, "o pedido no terminal", func() bool { return strings.Contains(lerArquivo(linhas), "frase curta para eu falar na reunião") })
	if _, quadro := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/perfis/1/quadro", nil); len(quadro["frases"].([]any)) != 1 {
		t.Errorf("frases no quadro: %v", quadro["frases"])
	}
	pedidos, _ := servidor.Banco.PedidosAbertos(context.Background(), 1)
	if len(pedidos) != 1 {
		t.Fatalf("pedidos abertos: %v", pedidos)
	}
	// O agente responde pela nota e conclui: a frase vai também para a sprint.
	pedirComo(t, token, "PUT", srv.URL+"/v1/agente/nota", map[string]any{"texto": "Terminei a tela nova de pedidos.", "modo": "complementar", "pedido": pedidos[0].ID})
	pedirComo(t, token, "POST", srv.URL+"/v1/agente/pedidos/"+itoa(int(pedidos[0].ID))+"/concluir", map[string]any{})
	eventos.esperar("frase escrita", frase(false))
	if d, s := notaDeHoje(t, servidor, 1); d != "Terminei a tela nova de pedidos." || s != d {
		t.Errorf("frase do agente: %q / %q", d, s)
	}
}

func TestFraseSemRespostaNoPrazo(t *testing.T) {
	claudeQueAnota(t)
	antes := PrazoFrase
	PrazoFrase = 300 * time.Millisecond
	t.Cleanup(func() { PrazoFrase = antes })
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	eventos := ouvirComo(t, tokenTela, srv.URL, "1")
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/agentes/1/mensagens", map[string]string{"texto": "trocar a cor do botão de salvar"})

	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/1", map[string]string{"coluna": "concluido"})
	eventos.esperar("escrevendo a frase", frase(true))
	eventos.esperar("desistiu de esperar", frase(false))
	if d, s := notaDeHoje(t, servidor, 1); d != "Concluí a tarefa “Nova tela de pedidos”. O último pedido foi: “trocar a cor do botão de salvar”." || s != d {
		t.Errorf("frase montada: %q / %q", d, s)
	}
	if abertos, _ := servidor.Banco.PedidosAbertos(context.Background(), 1); len(abertos) != 0 {
		t.Errorf("o pedido sem resposta ficou aberto: %v", abertos)
	}
}

func TestFraseJaGravadaPeloAgenteNaoPedeDeNovo(t *testing.T) {
	_, linhas := claudeQueAnota(t)
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	token := lerArquivo(servidor.DirCanal + "/agentes/1.token")

	// Tarefa 1: o agente gravou a frase ao terminar, como as instruções pedem.
	pedirComo(t, token, "PUT", srv.URL+"/v1/agente/nota", map[string]any{"texto": "Deixei pronta a tela nova de pedidos para a equipe de vendas.", "modo": "complementar"})
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/1", map[string]string{"coluna": "revisao"})
	// Tarefa 2: ninguém gravou, entra a reserva.
	pedirComo(t, tokenTela, "PATCH", srv.URL+"/v1/tarefas/2", map[string]string{"coluna": "concluido"})

	esperarAte(t, "a frase do agente na sprint", func() bool { _, s := notaDeHoje(t, servidor, 1); return s != "" })
	esperarAte(t, "a frase montada", func() bool { _, s := notaDeHoje(t, servidor, 2); return s != "" })
	time.Sleep(200 * time.Millisecond)
	if d, s := notaDeHoje(t, servidor, 1); d != "Deixei pronta a tela nova de pedidos para a equipe de vendas." || s != d {
		t.Errorf("frase do agente: %q / %q", d, s)
	}
	if abertos, _ := servidor.Banco.PedidosAbertos(context.Background(), 1); len(abertos) != 0 || strings.Contains(lerArquivo(linhas), "reunião") {
		t.Errorf("pediu a frase de novo: %v", abertos)
	}
	if d, s := notaDeHoje(t, servidor, 2); d != "Concluí a tarefa “Relatório do cliente-x”." || s != d {
		t.Errorf("reserva: %q / %q", d, s)
	}
}
