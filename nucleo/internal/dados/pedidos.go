package dados

import (
	"context"
	"database/sql"
	"errors"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/segredos"
)

// MaxPedido em caracteres.
const MaxPedido = 2000

// Estados de um pedido ao agente.
const (
	PedidoFila       = "fila"
	PedidoEntregue   = "entregue"
	PedidoRespondido = "respondido"
	PedidoCancelado  = "cancelado"
	PedidoFalhou     = "falhou"
)

// esquemaPedidos: pedidos que você fez ao agente de uma tarefa pela daily, pela
// sprint ou pela apresentação. Ficam fora da corrente de eventos e podem ser
// apagados, como as notas (decisão 0006): os eventos levam só o tamanho do
// texto.
const esquemaPedidos = `
CREATE TABLE IF NOT EXISTS pedidos (
	id INTEGER PRIMARY KEY,
	tarefa_id INTEGER NOT NULL REFERENCES tarefas(id) ON DELETE CASCADE,
	agente_id INTEGER REFERENCES agentes(id) ON DELETE SET NULL,
	tipo TEXT NOT NULL CHECK (tipo IN ('daily', 'sprint')),
	periodo TEXT NOT NULL,
	texto TEXT NOT NULL,
	estado TEXT NOT NULL CHECK (estado IN ('fila', 'entregue', 'respondido', 'cancelado', 'falhou')),
	motivo TEXT NOT NULL DEFAULT '',
	criado_em TEXT NOT NULL,
	entregue_em TEXT NOT NULL DEFAULT '',
	respondido_em TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS pedidos_por_tarefa ON pedidos (tarefa_id, id);
CREATE INDEX IF NOT EXISTS pedidos_por_agente ON pedidos (agente_id, estado);
`

// Pedido é algo a mais que você pediu ao agente da tarefa; a resposta volta
// para a nota (tipo e período) pelas ferramentas da Colmeia.
type Pedido struct {
	ID           int64  `json:"id"`
	TarefaID     int64  `json:"tarefa_id"`
	AgenteID     int64  `json:"agente_id,omitempty"`
	Tipo         string `json:"tipo"`
	Periodo      string `json:"periodo"`
	Texto        string `json:"texto"`
	Estado       string `json:"estado"`
	Motivo       string `json:"motivo,omitempty"`
	CriadoEm     string `json:"criado_em"`
	EntregueEm   string `json:"entregue_em,omitempty"`
	RespondidoEm string `json:"respondido_em,omitempty"`
	// Horas locais ("21:40") para a tela, que não lida com fuso.
	EntregueHora   string `json:"entregue_hora,omitempty"`
	RespondidoHora string `json:"respondido_hora,omitempty"`
}

func horaLocal(momento string) string {
	t, err := time.Parse(time.RFC3339Nano, momento)
	if err != nil {
		return ""
	}
	return t.Local().Format("15:04")
}

func (p *Pedido) preencherHoras() {
	p.EntregueHora, p.RespondidoHora = horaLocal(p.EntregueEm), horaLocal(p.RespondidoEm)
}

// Aberto diz se o pedido ainda espera o agente.
func (p Pedido) Aberto() bool { return p.Estado == PedidoFila || p.Estado == PedidoEntregue }

const colunasPedido = `id, tarefa_id, COALESCE(agente_id, 0), tipo, periodo, texto, estado, motivo, criado_em, entregue_em, respondido_em`

func escanearPedido(l escaneavel, p *Pedido) error {
	err := l.Scan(&p.ID, &p.TarefaID, &p.AgenteID, &p.Tipo, &p.Periodo, &p.Texto, &p.Estado, &p.Motivo, &p.CriadoEm, &p.EntregueEm, &p.RespondidoEm)
	p.preencherHoras()
	return err
}

// TextoDePedido confere o texto de um pedido: não vazio, até MaxPedido, sem
// caracteres de controle (vai para o terminal do agente) e sem nada que
// pareça senha ou chave.
func TextoDePedido(texto string) (string, error) {
	texto = strings.TrimSpace(texto)
	if texto == "" {
		return "", ErrInvalido{"o pedido está vazio"}
	}
	if !utf8.ValidString(texto) || strings.ContainsFunc(texto, func(r rune) bool { return (r < ' ' && r != '\n' && r != '\t') || r == 0x7f }) {
		return "", ErrInvalido{"o pedido tem caracteres de controle"}
	}
	if utf8.RuneCountInString(texto) > MaxPedido {
		return "", ErrInvalido{"o pedido pode ter no máximo 2.000 caracteres"}
	}
	if segredos.Parece(texto) {
		return "", ErrInvalido{"Não enviei: parece ter uma senha ou chave"}
	}
	return texto, nil
}

// conteudoPedido é o que vai nos eventos de um pedido: nunca o texto, só o tamanho.
func conteudoPedido(p Pedido, titulo, projeto string) map[string]any {
	c := map[string]any{"pedido": p.ID, "tarefa": p.TarefaID, "titulo": titulo, "projeto_nome": projeto,
		"tipo": p.Tipo, "periodo": p.Periodo, "tamanho": utf8.RuneCountInString(p.Texto)}
	if p.AgenteID != 0 {
		c["agente"] = p.AgenteID
	}
	if p.Motivo != "" {
		c["motivo"] = p.Motivo
	}
	return c
}

// escopoDaTarefa lê o título, o projeto e o perfil da tarefa numa transação.
func escopoDaTarefa(ctx context.Context, tx *transacao, tarefa int64) (Escopo, string, string, error) {
	var projeto int64
	var titulo string
	err := tx.QueryRowContext(ctx, `SELECT projeto_id, titulo FROM tarefas WHERE id = ?`, tarefa).Scan(&projeto, &titulo)
	if errors.Is(err, sql.ErrNoRows) {
		return Escopo{}, "", "", ErrNaoEncontrado
	}
	if err != nil {
		return Escopo{}, "", "", err
	}
	perfil, nome, err := donoDoProjeto(ctx, tx, projeto)
	return Escopo{Perfil: perfil, Projeto: projeto, Tarefa: tarefa}, titulo, nome, err
}

// CriarPedido põe o pedido na fila do agente (que precisa ser da tarefa).
func (b *Banco) CriarPedido(ctx context.Context, tarefa, agente int64, tipo, periodo, texto string) (Pedido, error) {
	if err := PeriodoNotaValido(tipo, periodo); err != nil {
		return Pedido{}, err
	}
	texto, err := TextoDePedido(texto)
	if err != nil {
		return Pedido{}, err
	}
	p := Pedido{TarefaID: tarefa, AgenteID: agente, Tipo: tipo, Periodo: periodo, Texto: texto, Estado: PedidoFila, CriadoEm: agora()}
	err = b.emTransacao(ctx, func(tx *transacao) error {
		escopo, titulo, nome, err := escopoDaTarefa(ctx, tx, tarefa)
		if err != nil {
			return err
		}
		var dono int64
		err = tx.QueryRowContext(ctx, `SELECT tarefa_id FROM agentes WHERE id = ?`, agente).Scan(&dono)
		if errors.Is(err, sql.ErrNoRows) || (err == nil && dono != tarefa) {
			return ErrInvalido{"o agente não é desta tarefa"}
		}
		if err != nil {
			return err
		}
		r, err := tx.ExecContext(ctx, `INSERT INTO pedidos (tarefa_id, agente_id, tipo, periodo, texto, estado, criado_em) VALUES (?, ?, ?, ?, ?, ?, ?)`,
			tarefa, agente, tipo, periodo, texto, p.Estado, p.CriadoEm)
		if err != nil {
			return err
		}
		p.ID, _ = r.LastInsertId()
		escopo.Agente = agente
		return registrar(ctx, tx, "pedido.criado", escopo, conteudoPedido(p, titulo, nome))
	})
	return p, err
}

// Pedido lê um pedido pelo id.
func (b *Banco) Pedido(ctx context.Context, id int64) (Pedido, error) {
	var p Pedido
	err := escanearPedido(b.db.QueryRowContext(ctx, `SELECT `+colunasPedido+` FROM pedidos WHERE id = ?`, id), &p)
	if errors.Is(err, sql.ErrNoRows) {
		return p, ErrNaoEncontrado
	}
	return p, err
}

func (b *Banco) listarPedidos(ctx context.Context, consulta string, args ...any) ([]Pedido, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT `+colunasPedido+` FROM pedidos WHERE `+consulta, args...)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []Pedido{}
	for linhas.Next() {
		var p Pedido
		if err := escanearPedido(linhas, &p); err != nil {
			return nil, err
		}
		lista = append(lista, p)
	}
	return lista, linhas.Err()
}

// ListarPedidos traz os últimos 50 pedidos da tarefa, do mais novo ao mais
// antigo; com tipo e período, só os daquela nota.
func (b *Banco) ListarPedidos(ctx context.Context, tarefa int64, tipo, periodo string) ([]Pedido, error) {
	if tipo == "" {
		return b.listarPedidos(ctx, `tarefa_id = ? ORDER BY id DESC LIMIT 50`, tarefa)
	}
	if err := PeriodoNotaValido(tipo, periodo); err != nil {
		return nil, err
	}
	return b.listarPedidos(ctx, `tarefa_id = ? AND tipo = ? AND periodo = ? ORDER BY id DESC LIMIT 50`, tarefa, tipo, periodo)
}

// PedidosAbertos traz os pedidos na fila ou com o agente, do mais antigo ao mais novo.
func (b *Banco) PedidosAbertos(ctx context.Context, tarefa int64) ([]Pedido, error) {
	return b.listarPedidos(ctx, `tarefa_id = ? AND estado IN ('fila', 'entregue') ORDER BY id`, tarefa)
}

// PedidosAbertosDoAgente traz os pedidos do agente na fila ou com ele.
func (b *Banco) PedidosAbertosDoAgente(ctx context.Context, agente int64) ([]Pedido, error) {
	return b.listarPedidos(ctx, `agente_id = ? AND estado IN ('fila', 'entregue') ORDER BY id`, agente)
}

// PedidosAbertosDoPerfil traz os pedidos abertos de todas as tarefas do perfil.
func (b *Banco) PedidosAbertosDoPerfil(ctx context.Context, perfil int64) ([]Pedido, error) {
	return b.listarPedidos(ctx, `estado IN ('fila', 'entregue') AND tarefa_id IN (
		SELECT t.id FROM tarefas t JOIN projetos p ON p.id = t.projeto_id JOIN workspaces w ON w.id = p.workspace_id WHERE w.perfil_id = ?) ORDER BY id`, perfil)
}

// PedidosRecentesDoPerfil traz os pedidos abertos e os fechados desde
// `desde` (RFC 3339), de todas as tarefas do perfil: o retrato do quadro.
func (b *Banco) PedidosRecentesDoPerfil(ctx context.Context, perfil int64, desde string) ([]Pedido, error) {
	return b.listarPedidos(ctx, `(estado IN ('fila', 'entregue') OR respondido_em >= ?) AND tarefa_id IN (
		SELECT t.id FROM tarefas t JOIN projetos p ON p.id = t.projeto_id JOIN workspaces w ON w.id = p.workspace_id WHERE w.perfil_id = ?) ORDER BY id`, desde, perfil)
}

// TextosDosPedidos traz o texto dos pedidos do perfil pelo id, para a linha do
// tempo citar (o evento só tem o tamanho).
func (b *Banco) TextosDosPedidos(ctx context.Context, perfil int64) (map[int64]string, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT d.id, d.texto FROM pedidos d JOIN tarefas t ON t.id = d.tarefa_id
		JOIN projetos p ON p.id = t.projeto_id JOIN workspaces w ON w.id = p.workspace_id WHERE w.perfil_id = ?`, perfil)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	textos := map[int64]string{}
	for linhas.Next() {
		var id int64
		var texto string
		if err := linhas.Scan(&id, &texto); err != nil {
			return nil, err
		}
		textos[id] = texto
	}
	return textos, linhas.Err()
}

// ErrPedidoFechado: o pedido já não está no estado em que a mudança vale.
var ErrPedidoFechado = ErrInvalido{"o pedido não está mais aberto"}

// MudarPedido leva o pedido a um novo estado, só se ele estiver num dos
// estados `de`, e grava o evento pedido.<estado>. Devolve o pedido como ficou.
func (b *Banco) MudarPedido(ctx context.Context, id int64, de []string, para, motivo string) (Pedido, error) {
	if err := umDe("estado", para, []string{PedidoEntregue, PedidoRespondido, PedidoCancelado, PedidoFalhou}); err != nil {
		return Pedido{}, err
	}
	var p Pedido
	err := b.emTransacao(ctx, func(tx *transacao) error {
		err := escanearPedido(tx.QueryRowContext(ctx, `SELECT `+colunasPedido+` FROM pedidos WHERE id = ?`, id), &p)
		if errors.Is(err, sql.ErrNoRows) {
			return ErrNaoEncontrado
		}
		if err != nil {
			return err
		}
		valido := false
		for _, e := range de {
			valido = valido || e == p.Estado
		}
		if !valido {
			return ErrPedidoFechado
		}
		momento := agora()
		p.Estado, p.Motivo = para, motivo
		switch para {
		case PedidoEntregue:
			p.EntregueEm = momento
		default:
			p.RespondidoEm = momento
		}
		p.preencherHoras()
		if _, err := tx.ExecContext(ctx, `UPDATE pedidos SET estado = ?, motivo = ?, entregue_em = ?, respondido_em = ? WHERE id = ?`,
			p.Estado, p.Motivo, p.EntregueEm, p.RespondidoEm, id); err != nil {
			return err
		}
		escopo, titulo, nome, err := escopoDaTarefa(ctx, tx, p.TarefaID)
		if err != nil {
			return err
		}
		escopo.Agente = p.AgenteID
		return registrar(ctx, tx, "pedido."+para, escopo, conteudoPedido(p, titulo, nome))
	})
	return p, err
}
