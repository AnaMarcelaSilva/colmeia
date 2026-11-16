package dados

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"strings"
)

// A montagem de cada tarefa para a reunião (pacote montagem): o tópico e a
// frase do slide e os cartões da coluna na lousa da reunião. Fica fora da
// corrente de eventos, como as notas (decisão 0006): é refeita a cada
// mudança da tarefa e pode ser apagada. Uma por tarefa; a daily de hoje e a
// sprint montam as lousas delas a partir daqui. Os cartões ficam como o JSON
// do pacote montagem, que este pacote não importa (quem monta é a API).

// Montagem é a montagem guardada de uma tarefa.
type Montagem struct {
	TarefaID  int64           `json:"tarefa_id"`
	Topico    string          `json:"topico"`
	Frase     string          `json:"frase"`
	Cartoes   json.RawMessage `json:"cartoes"`
	Origem    string          `json:"origem"`
	Base      string          `json:"base,omitempty"`
	MontadaEm string          `json:"montada_em"`
}

const esquemaMontagens = `
CREATE TABLE IF NOT EXISTS montagens (
	tarefa_id INTEGER PRIMARY KEY REFERENCES tarefas(id) ON DELETE CASCADE,
	topico TEXT NOT NULL,
	frase TEXT NOT NULL,
	cartoes TEXT NOT NULL,
	origem TEXT NOT NULL CHECK (origem IN ('simples', 'montador')),
	base TEXT NOT NULL DEFAULT '',
	montada_em TEXT NOT NULL
);
`

// GuardarMontagem grava (ou troca) a montagem da tarefa.
func (b *Banco) GuardarMontagem(ctx context.Context, m Montagem) (Montagem, error) {
	if m.TarefaID <= 0 || strings.TrimSpace(m.Topico) == "" || len(m.Cartoes) == 0 || !json.Valid(m.Cartoes) {
		return m, ErrInvalido{"a montagem precisa da tarefa, do tópico e de um cartão"}
	}
	if m.Origem != "simples" && m.Origem != "montador" {
		return m, ErrInvalido{"origem da montagem desconhecida"}
	}
	cartoes := []byte(m.Cartoes)
	m.MontadaEm = agora()
	err := b.emTransacao(ctx, func(tx *transacao) error {
		var existe int64
		if err := tx.QueryRowContext(ctx, `SELECT id FROM tarefas WHERE id = ?`, m.TarefaID).Scan(&existe); errors.Is(err, sql.ErrNoRows) {
			return ErrNaoEncontrado
		} else if err != nil {
			return err
		}
		_, err := tx.ExecContext(ctx, `INSERT INTO montagens (tarefa_id, topico, frase, cartoes, origem, base, montada_em) VALUES (?, ?, ?, ?, ?, ?, ?)
			ON CONFLICT (tarefa_id) DO UPDATE SET topico = excluded.topico, frase = excluded.frase, cartoes = excluded.cartoes,
			origem = excluded.origem, base = excluded.base, montada_em = excluded.montada_em`,
			m.TarefaID, m.Topico, m.Frase, string(cartoes), m.Origem, m.Base, m.MontadaEm)
		return err
	})
	return m, err
}

const colunasMontagem = `tarefa_id, topico, frase, cartoes, origem, base, montada_em`

func escanearMontagem(l escaneavel) (Montagem, error) {
	var m Montagem
	var cartoes string
	if err := l.Scan(&m.TarefaID, &m.Topico, &m.Frase, &cartoes, &m.Origem, &m.Base, &m.MontadaEm); err != nil {
		return m, err
	}
	m.Cartoes = json.RawMessage(cartoes)
	return m, nil
}

// MontagemDaTarefa lê a montagem da tarefa (ok false se ainda não há).
func (b *Banco) MontagemDaTarefa(ctx context.Context, tarefa int64) (Montagem, bool, error) {
	m, err := escanearMontagem(b.db.QueryRowContext(ctx, `SELECT `+colunasMontagem+` FROM montagens WHERE tarefa_id = ?`, tarefa))
	if errors.Is(err, sql.ErrNoRows) {
		return m, false, nil
	}
	return m, err == nil, err
}

// MontagensDasTarefas lê as montagens das tarefas dadas, por tarefa.
func (b *Banco) MontagensDasTarefas(ctx context.Context, tarefas []int64) (map[int64]Montagem, error) {
	por := map[int64]Montagem{}
	if len(tarefas) == 0 {
		return por, nil
	}
	args := make([]any, len(tarefas))
	for i, t := range tarefas {
		args[i] = t
	}
	linhas, err := b.db.QueryContext(ctx, `SELECT `+colunasMontagem+` FROM montagens WHERE tarefa_id IN (?`+strings.Repeat(`, ?`, len(tarefas)-1)+`)`, args...)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	for linhas.Next() {
		m, err := escanearMontagem(linhas)
		if err != nil {
			return nil, err
		}
		por[m.TarefaID] = m
	}
	return por, linhas.Err()
}
