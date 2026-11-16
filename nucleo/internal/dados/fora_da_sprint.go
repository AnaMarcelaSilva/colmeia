package dados

import (
	"context"
	"database/sql"
	"errors"
)

// TirarDaSprint tira a tarefa da sprint fixa (ela não vai à reunião: sai da
// página, do texto copiado e da apresentação dessa sprint), ou a traz de
// volta com fora falso. A daily, o quadro, a linha do tempo e as outras
// sprints continuam com a tarefa. Uma tarefa já removida também sai: o
// perfil e o projeto dela vêm do evento da remoção. A tarefa precisa ser do
// perfil da sprint.
func (b *Banco) TirarDaSprint(ctx context.Context, sprint, tarefa int64, fora bool) error {
	return b.emTransacao(ctx, func(tx *transacao) error {
		s, err := lerSprint(tx.QueryRowContext(ctx, `SELECT id, perfil_id, inicio, fim FROM sprints WHERE id = ?`, sprint))
		if err != nil {
			return err
		}
		var projeto int64
		var titulo string
		err = tx.QueryRowContext(ctx, `SELECT projeto_id, titulo FROM tarefas WHERE id = ?`, tarefa).Scan(&projeto, &titulo)
		var perfil int64
		var nomeProjeto string
		switch {
		case errors.Is(err, sql.ErrNoRows):
			if perfil, projeto, nomeProjeto, titulo, err = tarefaRemovida(ctx, tx, tarefa); err != nil {
				return err
			}
		case err != nil:
			return err
		default:
			if perfil, nomeProjeto, err = donoDoProjeto(ctx, tx, projeto); err != nil {
				return err
			}
		}
		if perfil != s.Perfil {
			return ErrNaoEncontrado
		}
		var r sql.Result
		if fora {
			r, err = tx.ExecContext(ctx, `INSERT OR IGNORE INTO fora_da_sprint (sprint_id, tarefa_id) VALUES (?, ?)`, sprint, tarefa)
		} else {
			r, err = tx.ExecContext(ctx, `DELETE FROM fora_da_sprint WHERE sprint_id = ? AND tarefa_id = ?`, sprint, tarefa)
		}
		if err != nil {
			return err
		}
		if n, _ := r.RowsAffected(); n == 0 {
			return nil // já estava assim: nenhum evento
		}
		return registrar(ctx, tx, "sprint.fora", Escopo{Perfil: perfil, Projeto: projeto, Tarefa: tarefa},
			map[string]any{"sprint": sprint, "tarefa": tarefa, "titulo": titulo, "projeto_nome": nomeProjeto, "fora": fora})
	})
}

// ForaDaSprint traz as tarefas tiradas da sprint, inclusive as já removidas.
func (b *Banco) ForaDaSprint(ctx context.Context, sprint int64) (map[int64]bool, error) {
	fora := map[int64]bool{}
	linhas, err := b.db.QueryContext(ctx, `SELECT tarefa_id FROM fora_da_sprint WHERE sprint_id = ?`, sprint)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	for linhas.Next() {
		var id int64
		if err := linhas.Scan(&id); err != nil {
			return nil, err
		}
		fora[id] = true
	}
	return fora, linhas.Err()
}
