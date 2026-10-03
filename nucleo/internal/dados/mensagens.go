package dados

import (
	"context"
	"database/sql"
	"errors"
	"strings"
	"unicode/utf8"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/segredos"
)

const (
	// MaxMensagens guardadas por agente; as mais antigas saem.
	MaxMensagens = 200
	// MaxTextoMensagem em bytes.
	MaxTextoMensagem = 16 << 10
)

// Mensagem é um texto que você mandou a um agente pela caixa de mensagem.
type Mensagem struct {
	ID        int64  `json:"id"`
	Texto     string `json:"texto"`
	EnviadaEm string `json:"enviada_em"`
}

func textoDeMensagem(texto string) (string, error) {
	if len(texto) > MaxTextoMensagem {
		return "", ErrInvalido{"a mensagem pode ter no máximo 16 KB"}
	}
	if !utf8.ValidString(texto) {
		return "", ErrInvalido{"a mensagem não é um texto válido"}
	}
	if strings.ContainsFunc(texto, func(r rune) bool { return (r < ' ' && r != '\n' && r != '\t') || r == 0x7f }) {
		return "", ErrInvalido{"a mensagem tem caracteres de controle"}
	}
	if strings.TrimSpace(texto) == "" {
		return "", ErrInvalido{"a mensagem está vazia"}
	}
	return texto, nil
}

// GuardarMensagem acrescenta o texto ao histórico do agente e diz se guardou.
// Não guarda o que parece ter um segredo, nem a repetição da última mensagem
// (como o ignoredups do bash); passou de MaxMensagens, as mais antigas saem.
func (b *Banco) GuardarMensagem(ctx context.Context, agente int64, texto string) (bool, error) {
	texto, err := textoDeMensagem(texto)
	if err != nil {
		return false, err
	}
	if _, err := b.Agente(ctx, agente); err != nil {
		return false, err
	}
	if segredos.Parece(texto) {
		return false, nil
	}
	guardada := false
	err = b.emTransacao(ctx, func(tx *transacao) error {
		var ultima string
		err := tx.QueryRowContext(ctx, `SELECT texto FROM mensagens WHERE agente_id = ? ORDER BY id DESC LIMIT 1`, agente).Scan(&ultima)
		if err != nil && !errors.Is(err, sql.ErrNoRows) {
			return err
		}
		if err == nil && ultima == texto {
			guardada = true
			return nil
		}
		if _, err := tx.ExecContext(ctx, `INSERT INTO mensagens (agente_id, texto, enviada_em) VALUES (?, ?, ?)`, agente, texto, agora()); err != nil {
			return err
		}
		_, err = tx.ExecContext(ctx, `DELETE FROM mensagens WHERE agente_id = ? AND id NOT IN (SELECT id FROM mensagens WHERE agente_id = ? ORDER BY id DESC LIMIT ?)`,
			agente, agente, MaxMensagens)
		guardada = err == nil
		return err
	})
	return guardada, err
}

// ListarMensagens traz o histórico do agente, da mais nova para a mais antiga.
func (b *Banco) ListarMensagens(ctx context.Context, agente int64) ([]Mensagem, error) {
	if _, err := b.Agente(ctx, agente); err != nil {
		return nil, err
	}
	linhas, err := b.db.QueryContext(ctx, `SELECT id, texto, enviada_em FROM mensagens WHERE agente_id = ? ORDER BY id DESC LIMIT ?`, agente, MaxMensagens)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []Mensagem{}
	for linhas.Next() {
		var m Mensagem
		if err := linhas.Scan(&m.ID, &m.Texto, &m.EnviadaEm); err != nil {
			return nil, err
		}
		lista = append(lista, m)
	}
	return lista, linhas.Err()
}

// LimparMensagens apaga o histórico do agente e diz quantas eram.
func (b *Banco) LimparMensagens(ctx context.Context, agente int64) (int64, error) {
	if _, err := b.Agente(ctx, agente); err != nil {
		return 0, err
	}
	r, err := b.db.ExecContext(ctx, `DELETE FROM mensagens WHERE agente_id = ?`, agente)
	if err != nil {
		return 0, err
	}
	return r.RowsAffected()
}
