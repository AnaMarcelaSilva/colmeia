package dados

import (
	"context"
	"database/sql"
	"errors"
	"strings"
)

// MaxNomeAssunto e MaxAssuntos limitam os assuntos de uma sprint.
const (
	MaxNomeAssunto = 80
	MaxAssuntos    = 30
)

// Assunto junta tarefas de projetos diferentes numa sprint, para a reunião.
// O projeto das tarefas não muda; o assunto só vale nesta sprint.
type Assunto struct {
	ID      int64  `json:"id"`
	Sprint  int64  `json:"sprint_id"`
	Nome    string `json:"nome"`
	Posicao int    `json:"posicao"`
}

// AssuntosDaSprint traz os assuntos na ordem e o assunto de cada tarefa.
func (b *Banco) AssuntosDaSprint(ctx context.Context, sprint int64) ([]Assunto, map[int64]int64, error) {
	assuntos := []Assunto{}
	linhas, err := b.db.QueryContext(ctx, `SELECT id, sprint_id, nome, posicao FROM assuntos_sprint WHERE sprint_id = ? ORDER BY posicao, id`, sprint)
	if err != nil {
		return nil, nil, err
	}
	defer linhas.Close()
	for linhas.Next() {
		var a Assunto
		if err := linhas.Scan(&a.ID, &a.Sprint, &a.Nome, &a.Posicao); err != nil {
			return nil, nil, err
		}
		assuntos = append(assuntos, a)
	}
	if err := linhas.Err(); err != nil {
		return nil, nil, err
	}
	da := map[int64]int64{}
	tarefas, err := b.db.QueryContext(ctx, `SELECT tarefa_id, assunto_id FROM tarefas_assunto WHERE sprint_id = ?`, sprint)
	if err != nil {
		return nil, nil, err
	}
	defer tarefas.Close()
	for tarefas.Next() {
		var t, a int64
		if err := tarefas.Scan(&t, &a); err != nil {
			return nil, nil, err
		}
		da[t] = a
	}
	return assuntos, da, tarefas.Err()
}

func lerAssunto(linha interface{ Scan(...any) error }) (Assunto, error) {
	var a Assunto
	err := linha.Scan(&a.ID, &a.Sprint, &a.Nome, &a.Posicao)
	if errors.Is(err, sql.ErrNoRows) {
		return a, ErrNaoEncontrado
	}
	return a, err
}

// AssuntoPorID traz um assunto (para a API conferir o perfil da sprint).
func (b *Banco) AssuntoPorID(ctx context.Context, id int64) (Assunto, error) {
	return lerAssunto(b.db.QueryRowContext(ctx, `SELECT id, sprint_id, nome, posicao FROM assuntos_sprint WHERE id = ?`, id))
}

// CriarAssunto põe um assunto no fim da lista da sprint.
func (b *Banco) CriarAssunto(ctx context.Context, sprint int64, nome string) (Assunto, error) {
	nome, err := nomeValido("o assunto", strings.TrimSpace(nome), MaxNomeAssunto)
	if err != nil {
		return Assunto{}, err
	}
	var a Assunto
	err = b.emTransacao(ctx, func(tx *transacao) error {
		if _, err := lerSprint(tx.QueryRowContext(ctx, `SELECT id, perfil_id, inicio, fim FROM sprints WHERE id = ?`, sprint)); err != nil {
			return err
		}
		var n, posicao int
		if err := tx.QueryRowContext(ctx, `SELECT COUNT(*), COALESCE(MAX(posicao), -1) + 1 FROM assuntos_sprint WHERE sprint_id = ?`, sprint).Scan(&n, &posicao); err != nil {
			return err
		}
		if n >= MaxAssuntos {
			return ErrInvalido{Motivo: "a sprint já tem assuntos demais"}
		}
		r, err := tx.ExecContext(ctx, `INSERT INTO assuntos_sprint (sprint_id, nome, posicao) VALUES (?, ?, ?)`, sprint, nome, posicao)
		if err != nil {
			return err
		}
		id, _ := r.LastInsertId()
		a = Assunto{ID: id, Sprint: sprint, Nome: nome, Posicao: posicao}
		return nil
	})
	return a, err
}

// RenomearAssunto muda o nome do assunto.
func (b *Banco) RenomearAssunto(ctx context.Context, id int64, nome string) (Assunto, error) {
	nome, err := nomeValido("o assunto", strings.TrimSpace(nome), MaxNomeAssunto)
	if err != nil {
		return Assunto{}, err
	}
	r, err := b.db.ExecContext(ctx, `UPDATE assuntos_sprint SET nome = ? WHERE id = ?`, nome, id)
	if err != nil {
		return Assunto{}, err
	}
	if n, _ := r.RowsAffected(); n == 0 {
		return Assunto{}, ErrNaoEncontrado
	}
	return b.AssuntoPorID(ctx, id)
}

// RemoverAssunto apaga o assunto; as tarefas dele voltam ao projeto.
func (b *Banco) RemoverAssunto(ctx context.Context, id int64) error {
	r, err := b.db.ExecContext(ctx, `DELETE FROM assuntos_sprint WHERE id = ?`, id)
	if err != nil {
		return err
	}
	if n, _ := r.RowsAffected(); n == 0 {
		return ErrNaoEncontrado
	}
	return nil
}

// DefinirAssunto põe as tarefas no assunto (0 tira do assunto). Arrastar um
// projeto inteiro manda todas as tarefas dele de uma vez. As tarefas e o
// assunto precisam ser da sprint e do perfil dela.
func (b *Banco) DefinirAssunto(ctx context.Context, sprint int64, tarefas []int64, assunto int64) error {
	if len(tarefas) == 0 || len(tarefas) > 500 {
		return ErrInvalido{Motivo: "tarefas inválidas"}
	}
	return b.emTransacao(ctx, func(tx *transacao) error {
		s, err := lerSprint(tx.QueryRowContext(ctx, `SELECT id, perfil_id, inicio, fim FROM sprints WHERE id = ?`, sprint))
		if err != nil {
			return err
		}
		if assunto != 0 {
			a, err := lerAssunto(tx.QueryRowContext(ctx, `SELECT id, sprint_id, nome, posicao FROM assuntos_sprint WHERE id = ?`, assunto))
			if err != nil {
				return err
			}
			if a.Sprint != sprint {
				return ErrNaoEncontrado
			}
		}
		for _, t := range tarefas {
			var projeto int64
			err := tx.QueryRowContext(ctx, `SELECT projeto_id FROM tarefas WHERE id = ?`, t).Scan(&projeto)
			if errors.Is(err, sql.ErrNoRows) {
				return ErrNaoEncontrado
			}
			if err != nil {
				return err
			}
			perfil, _, err := donoDoProjeto(ctx, tx, projeto)
			if err != nil {
				return err
			}
			if perfil != s.Perfil {
				return ErrNaoEncontrado
			}
			if assunto == 0 {
				_, err = tx.ExecContext(ctx, `DELETE FROM tarefas_assunto WHERE sprint_id = ? AND tarefa_id = ?`, sprint, t)
			} else {
				_, err = tx.ExecContext(ctx, `INSERT INTO tarefas_assunto (sprint_id, tarefa_id, assunto_id) VALUES (?, ?, ?)
					ON CONFLICT (sprint_id, tarefa_id) DO UPDATE SET assunto_id = excluded.assunto_id`, sprint, t, assunto)
			}
			if err != nil {
				return err
			}
		}
		return nil
	})
}

// RepetirAssuntos traz os assuntos da sprint anterior do perfil: os nomes
// que faltam e, para as tarefas ainda sem assunto, o que elas tinham lá.
// Devolve quantos assuntos vieram (0 sem sprint anterior com assuntos).
func (b *Banco) RepetirAssuntos(ctx context.Context, sprint int64) (int, error) {
	vieram := 0
	err := b.emTransacao(ctx, func(tx *transacao) error {
		s, err := lerSprint(tx.QueryRowContext(ctx, `SELECT id, perfil_id, inicio, fim FROM sprints WHERE id = ?`, sprint))
		if err != nil {
			return err
		}
		var anterior int64
		err = tx.QueryRowContext(ctx, `SELECT s.id FROM sprints s WHERE s.perfil_id = ? AND s.inicio < ?
			AND EXISTS (SELECT 1 FROM assuntos_sprint a WHERE a.sprint_id = s.id) ORDER BY s.inicio DESC LIMIT 1`, s.Perfil, s.Inicio).Scan(&anterior)
		if errors.Is(err, sql.ErrNoRows) {
			return nil
		}
		if err != nil {
			return err
		}
		linhas, err := tx.QueryContext(ctx, `SELECT id, nome FROM assuntos_sprint WHERE sprint_id = ? ORDER BY posicao, id`, anterior)
		if err != nil {
			return err
		}
		type par struct {
			id   int64
			nome string
		}
		var antigos []par
		for linhas.Next() {
			var p par
			if err := linhas.Scan(&p.id, &p.nome); err != nil {
				linhas.Close()
				return err
			}
			antigos = append(antigos, p)
		}
		linhas.Close()
		if err := linhas.Err(); err != nil {
			return err
		}
		for _, p := range antigos {
			var novo int64
			err := tx.QueryRowContext(ctx, `SELECT id FROM assuntos_sprint WHERE sprint_id = ? AND nome = ?`, sprint, p.nome).Scan(&novo)
			if errors.Is(err, sql.ErrNoRows) {
				var n, posicao int
				if err := tx.QueryRowContext(ctx, `SELECT COUNT(*), COALESCE(MAX(posicao), -1) + 1 FROM assuntos_sprint WHERE sprint_id = ?`, sprint).Scan(&n, &posicao); err != nil {
					return err
				}
				if n >= MaxAssuntos {
					break
				}
				r, err := tx.ExecContext(ctx, `INSERT INTO assuntos_sprint (sprint_id, nome, posicao) VALUES (?, ?, ?)`, sprint, p.nome, posicao)
				if err != nil {
					return err
				}
				novo, _ = r.LastInsertId()
				vieram++
			} else if err != nil {
				return err
			}
			if _, err := tx.ExecContext(ctx, `INSERT OR IGNORE INTO tarefas_assunto (sprint_id, tarefa_id, assunto_id)
				SELECT ?, tarefa_id, ? FROM tarefas_assunto WHERE sprint_id = ? AND assunto_id = ?`, sprint, novo, anterior, p.id); err != nil {
				return err
			}
		}
		return nil
	})
	return vieram, err
}
