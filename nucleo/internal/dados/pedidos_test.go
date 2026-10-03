package dados

import (
	"context"
	"errors"
	"strings"
	"testing"
)

func TestNotaComplementadaEVersao(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	tarefa, agente := tarefaComAgente(t, b)
	dia := "2026-10-02"

	// Complementar uma nota que não existe só grava o texto.
	n, err := b.GravarNota(ctx, GravacaoNota{Tarefa: tarefa.ID, Tipo: "daily", Periodo: dia, Texto: "Tela nova pronta", Modo: ModoComplementar, Agente: agente.ID})
	if err != nil || n.Texto != "Tela nova pronta" {
		t.Fatalf("primeira complementação: %+v %v", n, err)
	}
	n, err = b.GravarNota(ctx, GravacaoNota{Tarefa: tarefa.ID, Tipo: "daily", Periodo: dia, Texto: "Total de testes: 42\n", Modo: ModoComplementar, Agente: agente.ID})
	if err != nil || n.Texto != "Tela nova pronta\n\nTotal de testes: 42" {
		t.Fatalf("segunda complementação: %q %v", n.Texto, err)
	}
	// O evento diz que foi o agente, e não leva o texto.
	var conteudo string
	var agenteNoEvento int64
	b.db.QueryRow(`SELECT dados, agente_id FROM eventos WHERE tipo = 'nota.atualizada' ORDER BY id DESC LIMIT 1`).Scan(&conteudo, &agenteNoEvento)
	if agenteNoEvento != agente.ID || strings.Contains(conteudo, "42") || !strings.Contains(conteudo, `"modo":"complementar"`) {
		t.Errorf("evento da nota complementada: %s (agente %d)", conteudo, agenteNoEvento)
	}

	// A tela leu uma versão antiga: 409 com a nota atual.
	antiga := ""
	_, err = b.GravarNota(ctx, GravacaoNota{Tarefa: tarefa.ID, Tipo: "daily", Periodo: dia, Texto: "minha edição", Versao: &antiga})
	var mudou ErrNotaMudou
	if !errors.As(err, &mudou) || mudou.Atual.Texto != n.Texto || mudou.Atual.AtualizadaEm != n.AtualizadaEm {
		t.Fatalf("versão antiga: %v %+v", err, mudou)
	}
	// Com a versão certa, grava.
	if _, err := b.GravarNota(ctx, GravacaoNota{Tarefa: tarefa.ID, Tipo: "daily", Periodo: dia, Texto: "minha edição", Versao: &n.AtualizadaEm}); err != nil {
		t.Fatalf("versão certa: %v", err)
	}

	// Complementar além do limite volta um pedido claro para resumir.
	_, err = b.GravarNota(ctx, GravacaoNota{Tarefa: tarefa.ID, Tipo: "daily", Periodo: dia, Texto: strings.Repeat("a", MaxNota-5), Modo: ModoComplementar})
	if err == nil || !strings.Contains(err.Error(), "resuma") {
		t.Errorf("complementar além do limite: %v", err)
	}
	// Segredo também não entra por aqui.
	if _, err := b.GravarNota(ctx, GravacaoNota{Tarefa: tarefa.ID, Tipo: "daily", Periodo: dia, Texto: "token: abcdef123456", Modo: ModoComplementar}); !errors.As(err, &ErrInvalido{}) {
		t.Errorf("complemento com segredo: %v", err)
	}
	if lida, _ := b.Nota(ctx, tarefa.ID, "daily", dia); lida.Texto != "minha edição" {
		t.Errorf("nota depois das recusas: %q", lida.Texto)
	}
	if err := b.VerificarHistorico(ctx); err != nil {
		t.Error(err)
	}
}

func TestPedidosAoAgente(t *testing.T) {
	b, _ := bancoDeTeste(t)
	ctx := context.Background()
	tarefa, agente := tarefaComAgente(t, b)
	dia := "2026-10-02"

	for _, texto := range []string{"", "   ", "senha: hunter2222", "a\x1b[31m", strings.Repeat("á", MaxPedido+1)} {
		if _, err := b.CriarPedido(ctx, tarefa.ID, agente.ID, "daily", dia, texto); !errors.As(err, &ErrInvalido{}) {
			t.Errorf("pedido %q aceito: %v", texto, err)
		}
	}
	outra, _ := b.CriarTarefa(ctx, tarefa.ProjetoID, "Outra", "main", "")
	if _, err := b.CriarPedido(ctx, outra.ID, agente.ID, "daily", dia, "x"); !errors.As(err, &ErrInvalido{}) {
		t.Errorf("pedido com agente de outra tarefa: %v", err)
	}

	p, err := b.CriarPedido(ctx, tarefa.ID, agente.ID, "daily", dia, "  Traga o total de testes  ")
	if err != nil || p.Estado != PedidoFila || p.Texto != "Traga o total de testes" {
		t.Fatalf("criar: %+v %v", p, err)
	}
	if abertos, _ := b.PedidosAbertosDoAgente(ctx, agente.ID); len(abertos) != 1 {
		t.Errorf("abertos do agente: %+v", abertos)
	}
	p, err = b.MudarPedido(ctx, p.ID, []string{PedidoFila}, PedidoEntregue, "")
	if err != nil || p.EntregueEm == "" {
		t.Fatalf("entregar: %+v %v", p, err)
	}
	// Cancelar só vale na fila.
	if _, err := b.MudarPedido(ctx, p.ID, []string{PedidoFila}, PedidoCancelado, ""); !errors.Is(err, ErrPedidoFechado) {
		t.Errorf("cancelar entregue: %v", err)
	}
	if p, err = b.MudarPedido(ctx, p.ID, []string{PedidoFila, PedidoEntregue}, PedidoRespondido, ""); err != nil || p.RespondidoEm == "" {
		t.Fatalf("responder: %+v %v", p, err)
	}
	lista, _ := b.ListarPedidos(ctx, tarefa.ID, "daily", dia)
	if len(lista) != 1 || lista[0].Estado != PedidoRespondido {
		t.Errorf("lista: %+v", lista)
	}
	// Os eventos levam o tamanho, nunca o texto.
	linhas, _ := b.db.Query(`SELECT tipo, dados FROM eventos WHERE tipo LIKE 'pedido.%' ORDER BY id`)
	var tipos []string
	for linhas.Next() {
		var tipo, conteudo string
		linhas.Scan(&tipo, &conteudo)
		tipos = append(tipos, tipo)
		if strings.Contains(conteudo, "testes") || !strings.Contains(conteudo, `"tamanho":23`) {
			t.Errorf("evento %s: %s", tipo, conteudo)
		}
	}
	linhas.Close()
	if strings.Join(tipos, ",") != "pedido.criado,pedido.entregue,pedido.respondido" {
		t.Errorf("eventos: %v", tipos)
	}
	if textos, _ := b.TextosDosPedidos(ctx, 1); textos[p.ID] != "Traga o total de testes" {
		t.Errorf("textos: %v", textos)
	}

	// Remover o agente deixa o pedido sem agente; remover a tarefa leva os pedidos.
	if err := b.RemoverAgente(ctx, agente.ID); err != nil {
		t.Fatal(err)
	}
	if lido, _ := b.Pedido(ctx, p.ID); lido.AgenteID != 0 {
		t.Errorf("pedido depois de remover o agente: %+v", lido)
	}
	if err := b.RemoverTarefa(ctx, tarefa.ID); err != nil {
		t.Fatal(err)
	}
	if _, err := b.Pedido(ctx, p.ID); !errors.Is(err, ErrNaoEncontrado) {
		t.Errorf("pedido depois de remover a tarefa: %v", err)
	}
	if err := b.VerificarHistorico(ctx); err != nil {
		t.Error(err)
	}
}
