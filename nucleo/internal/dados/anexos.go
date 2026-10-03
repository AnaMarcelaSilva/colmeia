package dados

import (
	"cmp"
	"context"
	"database/sql"
	"errors"
	"path/filepath"
	"strings"
)

// Anexo é uma imagem ou um vídeo guardado pelo núcleo: captura de terminal,
// imagem colada na linha do tempo ou enviada numa mensagem a um agente, foto
// ou vídeo anexado na apresentação. O arquivo é
// <dados>/anexos/<perfil>/<sha256>.<formato>; a linha sobrevive à tarefa, como o evento.
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
	// Tipo é "imagem" ou "video"; Formato, a extensão do arquivo (png, mp4,
	// webm, mkv, mov); Nome, o nome original do arquivo, sem a pasta.
	Tipo    string `json:"tipo"`
	Formato string `json:"formato"`
	Nome    string `json:"nome,omitempty"`
}

var (
	OrigensAnexo = []string{"captura", "colagem", "mensagem", "arquivo"}
	TiposAnexo   = []string{"imagem", "video"}
	// FormatosAnexo são as únicas extensões que um anexo pode ter no disco.
	FormatosAnexo = []string{"png", "mp4", "webm", "mkv", "mov"}
)

// NomeDeArquivo guarda só o nome (sem pasta), validado como os outros nomes.
func NomeDeArquivo(nome string) (string, error) {
	if nome == "" {
		return "", nil
	}
	if strings.ContainsAny(nome, `/\`) || nome != filepath.Base(nome) || nome == "." || nome == ".." {
		return "", ErrInvalido{"o nome do arquivo não pode ter pastas"}
	}
	return nomeValido("O nome do arquivo", nome, 200)
}

// NovoAnexo é o que a API sabe ao receber a imagem; Agente diz de qual
// terminal veio uma captura.
type NovoAnexo struct {
	Perfil, Tarefa, Agente int64
	Sha256                 string
	Largura, Altura, Bytes int
	Origem, Legenda        string
	// Tipo vazio é "imagem"; Formato vazio é "png".
	Tipo, Formato, Nome string
	// Navegador: captura do navegador da tarefa (não do terminal).
	Navegador bool
}

func (b *Banco) CriarAnexo(ctx context.Context, n NovoAnexo) (Anexo, error) {
	if err := umDe("origem", n.Origem, OrigensAnexo); err != nil {
		return Anexo{}, err
	}
	n.Tipo, n.Formato = cmp.Or(n.Tipo, "imagem"), cmp.Or(n.Formato, "png")
	if err := umDe("tipo", n.Tipo, TiposAnexo); err != nil {
		return Anexo{}, err
	}
	if err := umDe("formato", n.Formato, FormatosAnexo); err != nil {
		return Anexo{}, err
	}
	nome, err := NomeDeArquivo(n.Nome)
	if err != nil {
		return Anexo{}, err
	}
	n.Nome = nome
	if n.Legenda != "" {
		legenda, err := nomeValido("A legenda", n.Legenda, 200)
		if err != nil {
			return Anexo{}, err
		}
		n.Legenda = legenda
	}
	a := Anexo{PerfilID: n.Perfil, TarefaID: n.Tarefa, Sha256: n.Sha256, Largura: n.Largura, Altura: n.Altura, Bytes: n.Bytes,
		Origem: n.Origem, Legenda: n.Legenda, CriadoEm: agora(), Tipo: n.Tipo, Formato: n.Formato, Nome: n.Nome}
	err = b.emTransacao(ctx, func(tx *transacao) error {
		escopo := Escopo{Perfil: n.Perfil}
		conteudo := map[string]any{"sha256": n.Sha256, "origem": n.Origem}
		if n.Tipo != "imagem" {
			conteudo["tipo"] = n.Tipo
		}
		if n.Navegador {
			conteudo["navegador"] = true
		}
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
		r, err := tx.ExecContext(ctx, `INSERT INTO anexos (perfil_id, tarefa_id, sha256, largura, altura, bytes, origem, legenda, criado_em, tipo, formato, nome)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			a.PerfilID, nulo(a.TarefaID), a.Sha256, a.Largura, a.Altura, a.Bytes, a.Origem, a.Legenda, a.CriadoEm, a.Tipo, a.Formato, a.Nome)
		if err != nil {
			return err
		}
		a.ID, _ = r.LastInsertId()
		conteudo["anexo"] = a.ID
		return registrar(ctx, tx, "anexo.adicionado", escopo, conteudo)
	})
	return a, err
}

const colunasAnexo = `id, perfil_id, COALESCE(tarefa_id, 0), sha256, largura, altura, bytes, origem, legenda, criado_em, removido, tipo, formato, nome`

func escanearAnexo(l escaneavel, a *Anexo) error {
	return l.Scan(&a.ID, &a.PerfilID, &a.TarefaID, &a.Sha256, &a.Largura, &a.Altura, &a.Bytes, &a.Origem, &a.Legenda, &a.CriadoEm, &a.Removido, &a.Tipo, &a.Formato, &a.Nome)
}

func (b *Banco) Anexo(ctx context.Context, id int64) (Anexo, error) {
	var a Anexo
	err := escanearAnexo(b.db.QueryRowContext(ctx, `SELECT `+colunasAnexo+` FROM anexos WHERE id = ?`, id), &a)
	if errors.Is(err, sql.ErrNoRows) {
		return a, ErrNaoEncontrado
	}
	return a, err
}

// AnexosDasTarefas traz os anexos não removidos das tarefas, criados entre
// desde e ate (RFC 3339, UTC; vazio não limita), do mais antigo ao mais novo.
// Uma consulta só, pelo índice de tarefa.
func (b *Banco) AnexosDasTarefas(ctx context.Context, tarefas []int64, desde, ate string) ([]Anexo, error) {
	lista := []Anexo{}
	if len(tarefas) == 0 {
		return lista, nil
	}
	consulta := `SELECT ` + colunasAnexo + ` FROM anexos WHERE removido = 0 AND tarefa_id IN (?` + strings.Repeat(`, ?`, len(tarefas)-1) + `)`
	args := make([]any, 0, len(tarefas)+2)
	for _, t := range tarefas {
		args = append(args, t)
	}
	if desde != "" {
		consulta += ` AND criado_em >= ?`
		args = append(args, desde)
	}
	if ate != "" {
		consulta += ` AND criado_em < ?`
		args = append(args, ate)
	}
	linhas, err := b.db.QueryContext(ctx, consulta+` ORDER BY id`, args...)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	for linhas.Next() {
		var a Anexo
		if err := escanearAnexo(linhas, &a); err != nil {
			return nil, err
		}
		lista = append(lista, a)
	}
	return lista, linhas.Err()
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
		if err := tx.QueryRowContext(ctx, `SELECT COUNT(*) FROM anexos WHERE perfil_id = ? AND sha256 = ? AND formato = ? AND removido = 0`, a.PerfilID, a.Sha256, a.Formato).Scan(&outros); err != nil {
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
