package dados

import (
	"context"
	"database/sql"
	"errors"
)

// TirarDaDaily tira a tarefa da daily do dia (AAAA-MM-DD), ou a traz de
// volta com fora falso. Só a daily daquele dia muda: a sprint, a linha do
// tempo e as dailies dos outros dias continuam com a tarefa.
func (b *Banco) TirarDaDaily(ctx context.Context, tarefa int64, dia string, fora bool) error {
	if err := PeriodoNotaValido("daily", dia); err != nil {
		return ErrInvalido{"o dia da daily precisa ser uma data AAAA-MM-DD"}
	}
	return b.emTransacao(ctx, func(tx *transacao) error {
		var projeto int64
		var titulo string
		err := tx.QueryRowContext(ctx, `SELECT projeto_id, titulo FROM tarefas WHERE id = ?`, tarefa).Scan(&projeto, &titulo)
		if errors.Is(err, sql.ErrNoRows) {
			return ErrNaoEncontrado
		}
		if err != nil {
			return err
		}
		perfil, nomeProjeto, err := donoDoProjeto(ctx, tx, projeto)
		if err != nil {
			return err
		}
		var r sql.Result
		if fora {
			r, err = tx.ExecContext(ctx, `INSERT OR IGNORE INTO fora_da_daily (tarefa_id, dia) VALUES (?, ?)`, tarefa, dia)
		} else {
			r, err = tx.ExecContext(ctx, `DELETE FROM fora_da_daily WHERE tarefa_id = ? AND dia = ?`, tarefa, dia)
		}
		if err != nil {
			return err
		}
		if n, _ := r.RowsAffected(); n == 0 {
			return nil // já estava assim: nenhum evento
		}
		return registrar(ctx, tx, "daily.fora", Escopo{Perfil: perfil, Projeto: projeto, Tarefa: tarefa},
			map[string]any{"tarefa": tarefa, "titulo": titulo, "projeto_nome": nomeProjeto, "dia": dia, "fora": fora})
	})
}

// ForaDaDaily traz as tarefas do perfil tiradas da daily do dia.
func (b *Banco) ForaDaDaily(ctx context.Context, perfil int64, dia string) (map[int64]bool, error) {
	fora := map[int64]bool{}
	linhas, err := b.db.QueryContext(ctx, `SELECT f.tarefa_id FROM fora_da_daily f
		JOIN tarefas t ON t.id = f.tarefa_id
		JOIN projetos p ON p.id = t.projeto_id
		JOIN workspaces w ON w.id = p.workspace_id
		WHERE f.dia = ? AND w.perfil_id = ?`, dia, perfil)
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
