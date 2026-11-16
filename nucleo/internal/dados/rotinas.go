package dados

// As rotinas de um workspace: pedidos que o Claude Code responde em texto a
// partir do quadro (e dos commits) de um período, quando você manda ou no
// horário da agenda. Cada vez que uma rotina roda vira uma rodada, com o
// texto que saiu (ou o erro). Quem roda é a API; aqui ficam os dados e as
// regras do que é uma rotina válida.

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/segredos"
)

// Limites de uma rotina.
const (
	MaxNomeRotina   = 80
	MaxPedidoRotina = 2000
	MaxDiasRotina   = 90
	// DiasDesdeATag em Dias: o período começa na última tag dos projetos.
	DiasDesdeATag  = -1
	MaxTextoRodada = 40000
)

const (
	OrigemAgenda  = "agenda"
	RodadaRodando = "rodando"
	RodadaOk      = "ok"
	RodadaErro    = "erro"
)

// Rotina é um pedido guardado do workspace. AgendaDias são os dias da semana
// (1 = segunda … 7 = domingo) e AgendaHora a hora local "17:00"; sem dias, a
// rotina só roda quando você manda. Dias é o período olhado (0: desde a
// última rodada que deu certo; DiasDesdeATag: desde a última tag de cada
// projeto git, e o quadro desde a tag mais antiga entre eles).
type Rotina struct {
	ID           int64  `json:"id"`
	WorkspaceID  int64  `json:"workspace_id"`
	Nome         string `json:"nome"`
	Pedido       string `json:"pedido"`
	DoQuadro     bool   `json:"do_quadro"`
	DoCodigo     bool   `json:"do_codigo"`
	Dias         int    `json:"dias"`
	AgendaDias   []int  `json:"agenda_dias"`
	AgendaHora   string `json:"agenda_hora"`
	Ativa        bool   `json:"ativa"`
	CriadoEm     string `json:"criado_em"`
	AtualizadoEm string `json:"atualizado_em"`
	// Ultima é a rodada mais recente (sem o texto), se houver.
	Ultima *Rodada `json:"ultima,omitempty"`
}

// Rodada é uma vez que a rotina rodou. Texto só vem quando pedido (a lista
// traz as rodadas sem ele).
type Rodada struct {
	ID         int64  `json:"id"`
	RotinaID   int64  `json:"rotina_id"`
	Momento    string `json:"momento"`
	Origem     string `json:"origem"`
	Estado     string `json:"estado"`
	De         string `json:"de"`
	Ate        string `json:"ate"`
	Texto      string `json:"texto,omitempty"`
	Resumo     string `json:"resumo"`
	Erro       string `json:"erro"`
	TerminouEm string `json:"terminou_em"`
	// MomentoTexto é o Momento como a tela mostra ("hoje às 09:12"); a API
	// preenche, não fica no banco.
	MomentoTexto string `json:"momento_texto,omitempty"`
}

// NovaRotina é o que a tela manda ao criar ou editar.
type NovaRotina struct {
	Nome       string `json:"nome"`
	Pedido     string `json:"pedido"`
	DoQuadro   bool   `json:"do_quadro"`
	DoCodigo   bool   `json:"do_codigo"`
	Dias       int    `json:"dias"`
	AgendaDias []int  `json:"agenda_dias"`
	AgendaHora string `json:"agenda_hora"`
	Ativa      bool   `json:"ativa"`
}

func rotinaValida(n NovaRotina) (NovaRotina, error) {
	nome, err := nomeValido("O nome da rotina", n.Nome, MaxNomeRotina)
	if err != nil {
		return n, err
	}
	n.Nome = nome
	n.Pedido = strings.TrimSpace(n.Pedido)
	switch {
	case n.Pedido == "":
		return n, ErrInvalido{"escreva o que a rotina deve produzir"}
	case len(n.Pedido) > MaxPedidoRotina:
		return n, ErrInvalido{fmt.Sprintf("o pedido pode ter no máximo %d caracteres", MaxPedidoRotina)}
	case segredos.Parece(n.Pedido):
		return n, ErrInvalido{"o pedido parece ter uma senha ou chave"}
	case !n.DoQuadro && !n.DoCodigo:
		return n, ErrInvalido{"a rotina precisa ler o quadro, os commits ou os dois"}
	case n.Dias != DiasDesdeATag && (n.Dias < 0 || n.Dias > MaxDiasRotina):
		return n, ErrInvalido{fmt.Sprintf("o período vai de 0 a %d dias, ou desde a última tag", MaxDiasRotina)}
	}
	vistos := map[int]bool{}
	dias := n.AgendaDias[:0:0]
	for _, d := range n.AgendaDias {
		if d < 1 || d > 7 {
			return n, ErrInvalido{"os dias da agenda vão de 1 (segunda) a 7 (domingo)"}
		}
		if !vistos[d] {
			vistos[d] = true
			dias = append(dias, d)
		}
	}
	n.AgendaDias = dias
	if len(dias) > 0 {
		if _, err := time.Parse("15:04", n.AgendaHora); err != nil {
			return n, ErrInvalido{"a hora da agenda é no formato 17:00"}
		}
	} else {
		n.AgendaHora = ""
	}
	return n, nil
}

func diasParaTexto(dias []int) string {
	partes := make([]string, len(dias))
	for i, d := range dias {
		partes[i] = strconv.Itoa(d)
	}
	return strings.Join(partes, ",")
}

func diasDeTexto(texto string) []int {
	if texto == "" {
		return []int{}
	}
	var dias []int
	for _, p := range strings.Split(texto, ",") {
		if d, err := strconv.Atoi(p); err == nil {
			dias = append(dias, d)
		}
	}
	return dias
}

const colunasRotina = `id, workspace_id, nome, pedido, do_quadro, do_codigo, dias, agenda_dias, agenda_hora, ativa, criado_em, atualizado_em`

func escanearRotina(l escaneavel, r *Rotina) error {
	var dias string
	if err := l.Scan(&r.ID, &r.WorkspaceID, &r.Nome, &r.Pedido, &r.DoQuadro, &r.DoCodigo, &r.Dias, &dias, &r.AgendaHora, &r.Ativa, &r.CriadoEm, &r.AtualizadoEm); err != nil {
		return err
	}
	r.AgendaDias = diasDeTexto(dias)
	return nil
}

const colunasRodada = `id, rotina_id, momento, origem, estado, de, ate, resumo, erro, terminou_em`

func escanearRodada(l escaneavel, r *Rodada) error {
	return l.Scan(&r.ID, &r.RotinaID, &r.Momento, &r.Origem, &r.Estado, &r.De, &r.Ate, &r.Resumo, &r.Erro, &r.TerminouEm)
}

// ListarRotinas devolve as rotinas do workspace, por nome, cada uma com a
// última rodada.
func (b *Banco) ListarRotinas(ctx context.Context, workspace int64) ([]Rotina, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT `+colunasRotina+` FROM rotinas WHERE workspace_id = ? ORDER BY nome COLLATE NOCASE, id`, workspace)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	rotinas := []Rotina{}
	for linhas.Next() {
		var r Rotina
		if err := escanearRotina(linhas, &r); err != nil {
			return nil, err
		}
		rotinas = append(rotinas, r)
	}
	if err := linhas.Err(); err != nil {
		return nil, err
	}
	for i := range rotinas {
		if err := b.comUltimaRodada(ctx, &rotinas[i]); err != nil {
			return nil, err
		}
	}
	return rotinas, nil
}

func (b *Banco) comUltimaRodada(ctx context.Context, r *Rotina) error {
	var ultima Rodada
	err := escanearRodada(b.db.QueryRowContext(ctx, `SELECT `+colunasRodada+` FROM rodadas WHERE rotina_id = ? ORDER BY id DESC LIMIT 1`, r.ID), &ultima)
	if errors.Is(err, sql.ErrNoRows) {
		r.Ultima = nil
		return nil
	}
	if err != nil {
		return err
	}
	r.Ultima = &ultima
	return nil
}

// Rotina devolve uma rotina com a última rodada.
func (b *Banco) Rotina(ctx context.Context, id int64) (Rotina, error) {
	var r Rotina
	err := escanearRotina(b.db.QueryRowContext(ctx, `SELECT `+colunasRotina+` FROM rotinas WHERE id = ?`, id), &r)
	if errors.Is(err, sql.ErrNoRows) {
		return r, ErrNaoEncontrado
	}
	if err != nil {
		return r, err
	}
	return r, b.comUltimaRodada(ctx, &r)
}

// RotinasAgendadas devolve todas as rotinas ativas com agenda, de todos os
// perfis, para o agendador.
func (b *Banco) RotinasAgendadas(ctx context.Context) ([]Rotina, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT `+colunasRotina+` FROM rotinas WHERE ativa = 1 AND agenda_dias <> '' ORDER BY id`)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	var rotinas []Rotina
	for linhas.Next() {
		var r Rotina
		if err := escanearRotina(linhas, &r); err != nil {
			return nil, err
		}
		rotinas = append(rotinas, r)
	}
	return rotinas, linhas.Err()
}

// PerfilDaRotina é o perfil dono do workspace da rotina.
func (b *Banco) PerfilDaRotina(ctx context.Context, id int64) (int64, error) {
	var perfil int64
	err := b.db.QueryRowContext(ctx, `SELECT w.perfil_id FROM rotinas r JOIN workspaces w ON w.id = r.workspace_id WHERE r.id = ?`, id).Scan(&perfil)
	if errors.Is(err, sql.ErrNoRows) {
		return 0, ErrNaoEncontrado
	}
	return perfil, err
}

func eventoDaRotina(r Rotina) map[string]any {
	return map[string]any{"rotina": r, "workspace_id": r.WorkspaceID}
}

// CriarRotina guarda uma rotina nova do workspace.
func (b *Banco) CriarRotina(ctx context.Context, workspace int64, n NovaRotina) (Rotina, error) {
	n, err := rotinaValida(n)
	if err != nil {
		return Rotina{}, err
	}
	perfil, err := b.donoDoWorkspace(ctx, workspace)
	if err != nil {
		return Rotina{}, err
	}
	var r Rotina
	err = b.emTransacao(ctx, func(tx *transacao) error {
		agora := agora()
		res, err := tx.ExecContext(ctx, `INSERT INTO rotinas (workspace_id, nome, pedido, do_quadro, do_codigo, dias, agenda_dias, agenda_hora, ativa, criado_em, atualizado_em)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`, workspace, n.Nome, n.Pedido, n.DoQuadro, n.DoCodigo, n.Dias, diasParaTexto(n.AgendaDias), n.AgendaHora, n.Ativa, agora, agora)
		if err != nil {
			return err
		}
		id, _ := res.LastInsertId()
		if err := escanearRotina(tx.QueryRowContext(ctx, `SELECT `+colunasRotina+` FROM rotinas WHERE id = ?`, id), &r); err != nil {
			return err
		}
		return registrar(ctx, tx, "rotina.criada", Escopo{Perfil: perfil}, eventoDaRotina(r))
	})
	return r, err
}

// EditarRotina troca tudo o que a tela manda.
func (b *Banco) EditarRotina(ctx context.Context, id int64, n NovaRotina) (Rotina, error) {
	n, err := rotinaValida(n)
	if err != nil {
		return Rotina{}, err
	}
	perfil, err := b.PerfilDaRotina(ctx, id)
	if err != nil {
		return Rotina{}, err
	}
	var r Rotina
	err = b.emTransacao(ctx, func(tx *transacao) error {
		if _, err := tx.ExecContext(ctx, `UPDATE rotinas SET nome = ?, pedido = ?, do_quadro = ?, do_codigo = ?, dias = ?, agenda_dias = ?, agenda_hora = ?, ativa = ?, atualizado_em = ? WHERE id = ?`,
			n.Nome, n.Pedido, n.DoQuadro, n.DoCodigo, n.Dias, diasParaTexto(n.AgendaDias), n.AgendaHora, n.Ativa, agora(), id); err != nil {
			return err
		}
		if err := escanearRotina(tx.QueryRowContext(ctx, `SELECT `+colunasRotina+` FROM rotinas WHERE id = ?`, id), &r); err != nil {
			return err
		}
		return registrar(ctx, tx, "rotina.atualizada", Escopo{Perfil: perfil}, eventoDaRotina(r))
	})
	if err != nil {
		return r, err
	}
	return r, b.comUltimaRodada(ctx, &r)
}

// RemoverRotina apaga a rotina e as rodadas dela.
func (b *Banco) RemoverRotina(ctx context.Context, id int64) error {
	r, err := b.Rotina(ctx, id)
	if err != nil {
		return err
	}
	perfil, err := b.PerfilDaRotina(ctx, id)
	if err != nil {
		return err
	}
	return b.emTransacao(ctx, func(tx *transacao) error {
		if _, err := tx.ExecContext(ctx, `DELETE FROM rotinas WHERE id = ?`, id); err != nil {
			return err
		}
		return registrar(ctx, tx, "rotina.removida", Escopo{Perfil: perfil}, map[string]any{"rotina_id": id, "workspace_id": r.WorkspaceID, "nome": r.Nome})
	})
}

// ComecarRodada abre uma rodada "rodando" da rotina, olhando [de, ate].
func (b *Banco) ComecarRodada(ctx context.Context, rotina int64, origem, de, ate string) (Rodada, error) {
	if err := umDe("origem", origem, []string{OrigemVoce, OrigemAgenda}); err != nil {
		return Rodada{}, err
	}
	r, err := b.Rotina(ctx, rotina)
	if err != nil {
		return Rodada{}, err
	}
	perfil, err := b.PerfilDaRotina(ctx, rotina)
	if err != nil {
		return Rodada{}, err
	}
	var rodada Rodada
	err = b.emTransacao(ctx, func(tx *transacao) error {
		res, err := tx.ExecContext(ctx, `INSERT INTO rodadas (rotina_id, momento, origem, estado, de, ate) VALUES (?, ?, ?, 'rodando', ?, ?)`, rotina, agora(), origem, de, ate)
		if err != nil {
			return err
		}
		id, _ := res.LastInsertId()
		if err := escanearRodada(tx.QueryRowContext(ctx, `SELECT `+colunasRodada+` FROM rodadas WHERE id = ?`, id), &rodada); err != nil {
			return err
		}
		return registrar(ctx, tx, "rodada.comecou", Escopo{Perfil: perfil}, map[string]any{"rodada": rodada, "workspace_id": r.WorkspaceID, "nome": r.Nome})
	})
	return rodada, err
}

// TerminarRodada fecha a rodada com o texto (ok) ou com o erro.
func (b *Banco) TerminarRodada(ctx context.Context, id int64, texto, resumo, erro string) (Rodada, error) {
	if len(texto) > MaxTextoRodada {
		texto = texto[:MaxTextoRodada]
	}
	estado := RodadaOk
	if erro != "" {
		estado = RodadaErro
	}
	var rodada Rodada
	var perfil, workspace int64
	var nome string
	err := b.db.QueryRowContext(ctx, `SELECT w.perfil_id, w.id, r.nome FROM rodadas d JOIN rotinas r ON r.id = d.rotina_id JOIN workspaces w ON w.id = r.workspace_id WHERE d.id = ?`, id).Scan(&perfil, &workspace, &nome)
	if errors.Is(err, sql.ErrNoRows) {
		return rodada, ErrNaoEncontrado
	}
	if err != nil {
		return rodada, err
	}
	err = b.emTransacao(ctx, func(tx *transacao) error {
		if _, err := tx.ExecContext(ctx, `UPDATE rodadas SET estado = ?, texto = ?, resumo = ?, erro = ?, terminou_em = ? WHERE id = ? AND estado = 'rodando'`, estado, texto, resumo, erro, agora(), id); err != nil {
			return err
		}
		if err := escanearRodada(tx.QueryRowContext(ctx, `SELECT `+colunasRodada+` FROM rodadas WHERE id = ?`, id), &rodada); err != nil {
			return err
		}
		return registrar(ctx, tx, "rodada.terminou", Escopo{Perfil: perfil}, map[string]any{"rodada": rodada, "workspace_id": workspace, "nome": nome})
	})
	return rodada, err
}

// ListarRodadas devolve as rodadas da rotina, da mais nova para a mais
// antiga, sem o texto.
func (b *Banco) ListarRodadas(ctx context.Context, rotina int64, max int) ([]Rodada, error) {
	if max <= 0 {
		max = 30
	}
	linhas, err := b.db.QueryContext(ctx, `SELECT `+colunasRodada+` FROM rodadas WHERE rotina_id = ? ORDER BY id DESC LIMIT ?`, rotina, max)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	rodadas := []Rodada{}
	for linhas.Next() {
		var r Rodada
		if err := escanearRodada(linhas, &r); err != nil {
			return nil, err
		}
		rodadas = append(rodadas, r)
	}
	return rodadas, linhas.Err()
}

// Rodada devolve uma rodada com o texto.
func (b *Banco) Rodada(ctx context.Context, id int64) (Rodada, error) {
	var r Rodada
	err := b.db.QueryRowContext(ctx, `SELECT `+colunasRodada+`, texto FROM rodadas WHERE id = ?`, id).Scan(
		&r.ID, &r.RotinaID, &r.Momento, &r.Origem, &r.Estado, &r.De, &r.Ate, &r.Resumo, &r.Erro, &r.TerminouEm, &r.Texto)
	if errors.Is(err, sql.ErrNoRows) {
		return r, ErrNaoEncontrado
	}
	return r, err
}

// UltimaRodada devolve a rodada mais recente da rotina com a origem dada
// (ou qualquer, com origem vazia); ErrNaoEncontrado sem rodada.
func (b *Banco) UltimaRodada(ctx context.Context, rotina int64, origem string) (Rodada, error) {
	var r Rodada
	err := escanearRodada(b.db.QueryRowContext(ctx, `SELECT `+colunasRodada+` FROM rodadas WHERE rotina_id = ? AND (? = '' OR origem = ?) ORDER BY id DESC LIMIT 1`, rotina, origem, origem), &r)
	if errors.Is(err, sql.ErrNoRows) {
		return r, ErrNaoEncontrado
	}
	return r, err
}

// RodadasPresas marca como erro as rodadas que ficaram "rodando" de um núcleo
// que fechou no meio (chamado ao abrir).
func (b *Banco) RodadasPresas(ctx context.Context) error {
	_, err := b.db.ExecContext(ctx, `UPDATE rodadas SET estado = 'erro', erro = 'a Colmeia fechou antes de terminar', terminou_em = ? WHERE estado = 'rodando'`, agora())
	return err
}
