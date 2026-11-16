package dados

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
)

// TirarDaDaily tira a tarefa da daily do dia (AAAA-MM-DD), ou a traz de
// volta com fora falso. Só a daily daquele dia muda: a sprint, a linha do
// tempo e as dailies dos outros dias continuam com a tarefa. Uma tarefa
// já removida também sai: ela segue na daily pelo histórico, e o perfil e
// o projeto dela vêm do evento da remoção.
func (b *Banco) TirarDaDaily(ctx context.Context, tarefa int64, dia string, fora bool) error {
	if err := PeriodoNotaValido("daily", dia); err != nil {
		return ErrInvalido{"o dia da daily precisa ser uma data AAAA-MM-DD"}
	}
	return b.emTransacao(ctx, func(tx *transacao) error {
		var projeto int64
		var titulo string
		err := tx.QueryRowContext(ctx, `SELECT projeto_id, titulo FROM tarefas WHERE id = ?`, tarefa).Scan(&projeto, &titulo)
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
		var r sql.Result
		if fora {
			r, err = tx.ExecContext(ctx, `INSERT OR IGNORE INTO fora_da_daily (tarefa_id, dia, perfil_id) VALUES (?, ?, ?)`, tarefa, dia, perfil)
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

// tarefaRemovida acha, pelo evento da remoção, o perfil, o projeto e o
// título de uma tarefa que não existe mais. Sem o evento, não encontrada.
func tarefaRemovida(ctx context.Context, tx *transacao, tarefa int64) (perfil, projeto int64, nomeProjeto, titulo string, err error) {
	var bruto string
	err = tx.QueryRowContext(ctx, `SELECT perfil_id, projeto_id, dados FROM eventos
		WHERE tipo = 'tarefa.removida' AND tarefa_id = ? ORDER BY id DESC LIMIT 1`, tarefa).Scan(&perfil, &projeto, &bruto)
	if errors.Is(err, sql.ErrNoRows) {
		return 0, 0, "", "", ErrNaoEncontrado
	}
	if err != nil {
		return 0, 0, "", "", err
	}
	var dados struct {
		Titulo      string `json:"titulo"`
		ProjetoNome string `json:"projeto_nome"`
	}
	if err := json.Unmarshal([]byte(bruto), &dados); err != nil {
		return 0, 0, "", "", err
	}
	return perfil, projeto, dados.ProjetoNome, dados.Titulo, nil
}

// ForaDaDaily traz as tarefas do perfil tiradas da daily do dia, inclusive
// as já removidas.
func (b *Banco) ForaDaDaily(ctx context.Context, perfil int64, dia string) (map[int64]bool, error) {
	fora := map[int64]bool{}
	linhas, err := b.db.QueryContext(ctx, `SELECT tarefa_id FROM fora_da_daily WHERE dia = ? AND perfil_id = ?`, dia, perfil)
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
