package dados

import (
	"context"
	"database/sql"
	"errors"
	"time"
)

// A pauta da reunião: a ordem em que as tarefas são faladas e quais já
// foram apresentadas. É por reunião (perfil, tipo daily ou sprint e a chave
// do período, a mesma das notas: o dia, ou "de..ate"), e não muda a
// tarefa. "Daily apresentada" guarda a hora em que a daily do dia foi
// fechada: a próxima daily começa dali, com o que ficou aberto, e os vistos
// de antes deixam de valer.

// O esquema da pauta (versão 16).
const esquemaPauta = `
-- A pauta de uma reunião: a ordem das tarefas e quais já foram apresentadas.
-- A chave é a do período das notas (o dia da daily, ou "de..ate").
CREATE TABLE IF NOT EXISTS pauta (
	perfil_id INTEGER NOT NULL REFERENCES perfis(id) ON DELETE CASCADE,
	tipo TEXT NOT NULL,
	chave TEXT NOT NULL,
	tarefa_id INTEGER NOT NULL,
	ordem INTEGER NOT NULL DEFAULT 0,
	apresentada_em TEXT,
	PRIMARY KEY (perfil_id, tipo, chave, tarefa_id)
);
-- A hora em que a daily do dia foi apresentada: a próxima começa dali.
CREATE TABLE IF NOT EXISTS daily_apresentada (
	perfil_id INTEGER NOT NULL REFERENCES perfis(id) ON DELETE CASCADE,
	dia TEXT NOT NULL,
	apresentada_em TEXT NOT NULL,
	PRIMARY KEY (perfil_id, dia)
);
`

// ItemPauta é o que a pauta sabe de uma tarefa.
type ItemPauta struct {
	TarefaID int64
	// Ordem começa em 1; 0 é "sem ordem" (vale a ordem padrão).
	Ordem int
	// ApresentadaEm é a hora (RFC 3339) em que foi marcada; vazio, não foi.
	ApresentadaEm string
}

// Pauta traz a pauta de uma reunião, por tarefa.
func (b *Banco) Pauta(ctx context.Context, perfil int64, tipo, chave string) (map[int64]ItemPauta, error) {
	if err := PeriodoNotaValido(tipo, chave); err != nil {
		return nil, err
	}
	linhas, err := b.db.QueryContext(ctx, `SELECT tarefa_id, ordem, apresentada_em FROM pauta WHERE perfil_id = ? AND tipo = ? AND chave = ?`, perfil, tipo, chave)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	itens := map[int64]ItemPauta{}
	for linhas.Next() {
		var i ItemPauta
		var em sql.NullString
		if err := linhas.Scan(&i.TarefaID, &i.Ordem, &em); err != nil {
			return nil, err
		}
		i.ApresentadaEm = em.String
		itens[i.TarefaID] = i
	}
	return itens, linhas.Err()
}

// OrdenarPauta guarda a ordem das tarefas de uma reunião (as que não
// estão na lista perdem a ordem). Registra pauta.mudou.
func (b *Banco) OrdenarPauta(ctx context.Context, perfil int64, tipo, chave string, tarefas []int64) error {
	if err := PeriodoNotaValido(tipo, chave); err != nil {
		return err
	}
	return b.emTransacao(ctx, func(tx *transacao) error {
		if err := perfilExiste(ctx, tx, perfil); err != nil {
			return err
		}
		if _, err := tx.ExecContext(ctx, `UPDATE pauta SET ordem = 0 WHERE perfil_id = ? AND tipo = ? AND chave = ?`, perfil, tipo, chave); err != nil {
			return err
		}
		for i, t := range tarefas {
			if _, err := tx.ExecContext(ctx, `INSERT INTO pauta (perfil_id, tipo, chave, tarefa_id, ordem) VALUES (?, ?, ?, ?, ?)
				ON CONFLICT (perfil_id, tipo, chave, tarefa_id) DO UPDATE SET ordem = excluded.ordem`, perfil, tipo, chave, t, i+1); err != nil {
				return err
			}
		}
		if _, err := tx.ExecContext(ctx, `DELETE FROM pauta WHERE perfil_id = ? AND tipo = ? AND chave = ? AND ordem = 0 AND apresentada_em IS NULL`, perfil, tipo, chave); err != nil {
			return err
		}
		return registrar(ctx, tx, "pauta.mudou", Escopo{Perfil: perfil}, map[string]any{"tipo": tipo, "chave": chave, "ordem": tarefas})
	})
}

// MarcarApresentada marca (ou desmarca) uma tarefa como já apresentada na
// reunião. Registra pauta.mudou com a tarefa.
func (b *Banco) MarcarApresentada(ctx context.Context, perfil int64, tipo, chave string, tarefa int64, apresentada bool) error {
	if err := PeriodoNotaValido(tipo, chave); err != nil {
		return err
	}
	return b.emTransacao(ctx, func(tx *transacao) error {
		if err := perfilExiste(ctx, tx, perfil); err != nil {
			return err
		}
		var em any
		if apresentada {
			em = time.Now().UTC().Format(time.RFC3339Nano)
		}
		if _, err := tx.ExecContext(ctx, `INSERT INTO pauta (perfil_id, tipo, chave, tarefa_id, ordem, apresentada_em) VALUES (?, ?, ?, ?, 0, ?)
			ON CONFLICT (perfil_id, tipo, chave, tarefa_id) DO UPDATE SET apresentada_em = excluded.apresentada_em`, perfil, tipo, chave, tarefa, em); err != nil {
			return err
		}
		if _, err := tx.ExecContext(ctx, `DELETE FROM pauta WHERE perfil_id = ? AND tipo = ? AND chave = ? AND tarefa_id = ? AND ordem = 0 AND apresentada_em IS NULL`, perfil, tipo, chave, tarefa); err != nil {
			return err
		}
		return registrar(ctx, tx, "pauta.mudou", Escopo{Perfil: perfil, Tarefa: tarefa}, map[string]any{"tipo": tipo, "chave": chave, "tarefa": tarefa, "apresentada": apresentada})
	})
}

// FecharDaily marca a daily do dia como apresentada agora: devolve a hora
// (RFC 3339). Os vistos daquele dia são apagados (a próxima daily começa
// limpa) e a ordem fica. Registra daily.apresentada.
func (b *Banco) FecharDaily(ctx context.Context, perfil int64, dia string) (string, error) {
	if err := PeriodoNotaValido("daily", dia); err != nil {
		return "", err
	}
	em := time.Now().UTC().Format(time.RFC3339Nano)
	err := b.emTransacao(ctx, func(tx *transacao) error {
		if err := perfilExiste(ctx, tx, perfil); err != nil {
			return err
		}
		if _, err := tx.ExecContext(ctx, `INSERT INTO daily_apresentada (perfil_id, dia, apresentada_em) VALUES (?, ?, ?)
			ON CONFLICT (perfil_id, dia) DO UPDATE SET apresentada_em = excluded.apresentada_em`, perfil, dia, em); err != nil {
			return err
		}
		if _, err := tx.ExecContext(ctx, `UPDATE pauta SET apresentada_em = NULL WHERE perfil_id = ? AND tipo = 'daily' AND chave = ?`, perfil, dia); err != nil {
			return err
		}
		if _, err := tx.ExecContext(ctx, `DELETE FROM pauta WHERE perfil_id = ? AND tipo = 'daily' AND chave = ? AND ordem = 0 AND apresentada_em IS NULL`, perfil, dia); err != nil {
			return err
		}
		return registrar(ctx, tx, "daily.apresentada", Escopo{Perfil: perfil}, map[string]any{"dia": dia, "em": em})
	})
	return em, err
}

// DailyApresentada: a hora em que a daily do dia foi fechada (vazio se não foi).
func (b *Banco) DailyApresentada(ctx context.Context, perfil int64, dia string) (string, error) {
	var em string
	err := b.db.QueryRowContext(ctx, `SELECT apresentada_em FROM daily_apresentada WHERE perfil_id = ? AND dia = ?`, perfil, dia).Scan(&em)
	if errors.Is(err, sql.ErrNoRows) {
		return "", nil
	}
	return em, err
}

// UltimaDailyApresentada: a hora da última daily fechada do perfil (a mais
// recente), ou zero.
func (b *Banco) UltimaDailyApresentada(ctx context.Context, perfil int64) (time.Time, error) {
	var em string
	err := b.db.QueryRowContext(ctx, `SELECT apresentada_em FROM daily_apresentada WHERE perfil_id = ? ORDER BY apresentada_em DESC LIMIT 1`, perfil).Scan(&em)
	if errors.Is(err, sql.ErrNoRows) {
		return time.Time{}, nil
	}
	if err != nil {
		return time.Time{}, err
	}
	return time.Parse(time.RFC3339Nano, em)
}

func perfilExiste(ctx context.Context, tx *transacao, perfil int64) error {
	var um int
	err := tx.QueryRowContext(ctx, `SELECT 1 FROM perfis WHERE id = ?`, perfil).Scan(&um)
	if errors.Is(err, sql.ErrNoRows) {
		return ErrNaoEncontrado
	}
	return err
}
