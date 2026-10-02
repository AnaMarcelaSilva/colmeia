package dados

import (
	"context"
	"database/sql"
	"errors"
)

// Anexo é uma imagem guardada pelo núcleo: captura de terminal, imagem colada
// na linha do tempo ou enviada numa mensagem a um agente. O arquivo é
// <dados>/anexos/<perfil>/<sha256>.png; a linha sobrevive à tarefa, como o evento.
type Anexo struct {
	ID       int64  `json:"id"`
	PerfilID int64  `json:"perfil_id"`
	TarefaID int64  `json:"tarefa_id,omitempty"`
	Sha256   string `json:"sha256"`
	Largura  int    `json:"largura"`
	Altura   int    `json:"altura"`
	Bytes    int    `json:"bytes"`
	Origem   string `json:"origem"`
	Legenda  string `json:"legenda"`
	CriadoEm string `json:"criado_em"`
	Removido bool   `json:"removido"`
}

var OrigensAnexo = []string{"captura", "colagem", "mensagem"}

// NovoAnexo é o que a API sabe ao receber a imagem; Agente diz de qual
// terminal veio uma captura.
type NovoAnexo struct {
	Perfil, Tarefa, Agente int64
	Sha256                 string
	Largura, Altura, Bytes int
	Origem, Legenda        string
}

func (b *Banco) CriarAnexo(ctx context.Context, n NovoAnexo) (Anexo, error) {
	if err := umDe("origem", n.Origem, OrigensAnexo); err != nil {
		return Anexo{}, err
	}
	if n.Legenda != "" {
		legenda, err := nomeValido("A legenda", n.Legenda, 200)
		if err != nil {
			return Anexo{}, err
		}
		n.Legenda = legenda
	}
	a := Anexo{PerfilID: n.Perfil, TarefaID: n.Tarefa, Sha256: n.Sha256, Largura: n.Largura, Altura: n.Altura, Bytes: n.Bytes,
		Origem: n.Origem, Legenda: n.Legenda, CriadoEm: agora()}
	err := b.emTransacao(ctx, func(tx *transacao) error {
		escopo := Escopo{Perfil: n.Perfil}
		conteudo := map[string]any{"sha256": n.Sha256, "origem": n.Origem}
		if n.Tarefa != 0 {
			var projeto int64
			var titulo string
			err := tx.QueryRowContext(ctx, `SELECT projeto_id, titulo FROM tarefas WHERE id = ?`, n.Tarefa).Scan(&projeto, &titulo)
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
			if perfil != n.Perfil {
				return ErrNaoEncontrado
			}
			escopo.Projeto, escopo.Tarefa = projeto, n.Tarefa
			conteudo["tarefa"], conteudo["titulo"], conteudo["projeto_nome"] = n.Tarefa, titulo, nomeProjeto
		}
		if n.Agente != 0 {
			var ferramenta, papel string
			err := tx.QueryRowContext(ctx, `SELECT ferramenta, papel FROM agentes WHERE id = ? AND tarefa_id = ?`, n.Agente, n.Tarefa).Scan(&ferramenta, &papel)
			if errors.Is(err, sql.ErrNoRows) {
				return ErrInvalido{"o agente não é desta tarefa"}
			}
			if err != nil {
				return err
			}
			escopo.Agente = n.Agente
			conteudo["agente"], conteudo["ferramenta"], conteudo["papel"] = n.Agente, ferramenta, papel
		}
		r, err := tx.ExecContext(ctx, `INSERT INTO anexos (perfil_id, tarefa_id, sha256, largura, altura, bytes, origem, legenda, criado_em) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			a.PerfilID, nulo(a.TarefaID), a.Sha256, a.Largura, a.Altura, a.Bytes, a.Origem, a.Legenda, a.CriadoEm)
		if err != nil {
			return err
		}
		a.ID, _ = r.LastInsertId()
		conteudo["anexo"] = a.ID
		return registrar(ctx, tx, "anexo.adicionado", escopo, conteudo)
	})
	return a, err
}

func (b *Banco) Anexo(ctx context.Context, id int64) (Anexo, error) {
	var a Anexo
	err := b.db.QueryRowContext(ctx, `SELECT id, perfil_id, COALESCE(tarefa_id, 0), sha256, largura, altura, bytes, origem, legenda, criado_em, removido FROM anexos WHERE id = ?`, id).
		Scan(&a.ID, &a.PerfilID, &a.TarefaID, &a.Sha256, &a.Largura, &a.Altura, &a.Bytes, &a.Origem, &a.Legenda, &a.CriadoEm, &a.Removido)
	if errors.Is(err, sql.ErrNoRows) {
		return a, ErrNaoEncontrado
	}
	return a, err
}

// RemoverAnexo marca o anexo como removido (o evento continua) e diz se o
// arquivo ainda é usado por outro anexo do perfil com a mesma imagem.
func (b *Banco) RemoverAnexo(ctx context.Context, id int64) (a Anexo, emUso bool, err error) {
	a, err = b.Anexo(ctx, id)
	if err != nil {
		return a, false, err
	}
	if a.Removido {
		return a, false, ErrNaoEncontrado
	}
	err = b.emTransacao(ctx, func(tx *transacao) error {
		if _, err := tx.ExecContext(ctx, `UPDATE anexos SET removido = 1 WHERE id = ?`, id); err != nil {
			return err
		}
		var outros int
		if err := tx.QueryRowContext(ctx, `SELECT COUNT(*) FROM anexos WHERE perfil_id = ? AND sha256 = ? AND removido = 0`, a.PerfilID, a.Sha256).Scan(&outros); err != nil {
			return err
		}
		emUso = outros > 0
		escopo := Escopo{Perfil: a.PerfilID, Tarefa: a.TarefaID}
		if a.TarefaID != 0 {
			var projeto int64
			if tx.QueryRowContext(ctx, `SELECT projeto_id FROM tarefas WHERE id = ?`, a.TarefaID).Scan(&projeto) == nil {
				escopo.Projeto = projeto
			}
		}
		return registrar(ctx, tx, "anexo.removido", escopo, map[string]any{"anexo": id})
	})
	return a, emUso, err
}

// AnexosRemovidos diz quais dos anexos do perfil já foram removidos.
func (b *Banco) AnexosRemovidos(ctx context.Context, perfil int64) (map[int64]bool, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT id FROM anexos WHERE perfil_id = ? AND removido = 1`, perfil)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	removidos := map[int64]bool{}
	for linhas.Next() {
		var id int64
		if err := linhas.Scan(&id); err != nil {
			return nil, err
		}
		removidos[id] = true
	}
	return removidos, linhas.Err()
}
