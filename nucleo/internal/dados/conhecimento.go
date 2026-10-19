package dados

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"strings"
	"unicode/utf8"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/segredos"
)

// Limites da base de conhecimento. O texto que o agente recebe ao começar
// tem MaxConhecimentoInicio caracteres (perto de 2 mil tokens); o que passar
// disso vai só pelo título, e o agente lê o resto com ler_conhecimento.
const (
	MaxTituloConhecimento = 80
	MaxTextoConhecimento  = 4000
	MaxConhecimentoInicio = 8000
	// Sugestões dos agentes esperando você, por workspace: um agente que
	// anota demais não enche a lista.
	MaxSugestoes = 20
)

const (
	EstadoAprovada = "aprovada"
	EstadoSugerida = "sugerida"
)

// Conhecimento é uma nota da base de conhecimento: do workspace inteiro
// (ProjetoID 0) ou de um projeto. Todo agente que começa ali recebe as
// aprovadas. A de um agente nasce sugerida, até você aprovar; Substitui
// aponta a nota que a sugestão quer mudar.
type Conhecimento struct {
	ID           int64  `json:"id"`
	WorkspaceID  int64  `json:"workspace_id"`
	ProjetoID    int64  `json:"projeto_id"`
	Titulo       string `json:"titulo"`
	Texto        string `json:"texto"`
	Origem       string `json:"origem"`
	Estado       string `json:"estado"`
	Substitui    int64  `json:"substitui"`
	AgenteID     int64  `json:"agente_id"`
	CriadaEm     string `json:"criada_em"`
	AtualizadaEm string `json:"atualizada_em"`
}

// NovoConhecimento é o pedido de uma nota. Com Projeto, a nota é do projeto;
// sem ele, do Workspace.
type NovoConhecimento struct {
	Workspace int64
	Projeto   int64
	Titulo    string
	Texto     string
	Origem    string
	Agente    int64
	Substitui int64
}

func conhecimentoValido(titulo, texto string) (string, string, error) {
	titulo, err := nomeValido("O título", titulo, MaxTituloConhecimento)
	if err != nil {
		return "", "", err
	}
	texto = strings.TrimSpace(strings.ReplaceAll(texto, "\r\n", "\n"))
	if texto == "" {
		return "", "", ErrInvalido{"o texto não pode ficar vazio"}
	}
	if utf8.RuneCountInString(texto) > MaxTextoConhecimento {
		return "", "", ErrInvalido{fmt.Sprintf("o texto pode ter no máximo %d caracteres", MaxTextoConhecimento)}
	}
	if !utf8.ValidString(texto) || strings.ContainsFunc(texto, func(r rune) bool { return (r < ' ' && r != '\n' && r != '\t') || r == 0x7f }) {
		return "", "", ErrInvalido{"o texto tem caracteres de controle"}
	}
	// Vai para todo agente que começar: nada que pareça senha ou chave.
	if segredos.Parece(titulo) || segredos.Parece(texto) {
		return "", "", ErrInvalido{"a nota parece ter uma senha ou chave"}
	}
	return titulo, texto, nil
}

const colunasConhecimento = `c.id, COALESCE(c.workspace_id, p.workspace_id), COALESCE(c.projeto_id, 0), c.titulo, c.texto,
	c.origem, c.estado, COALESCE(c.substitui, 0), c.agente_id, c.criada_em, c.atualizada_em`

const deConhecimento = ` FROM conhecimento c LEFT JOIN projetos p ON p.id = c.projeto_id`

func lerConhecimento(linhas *sql.Rows) ([]Conhecimento, error) {
	defer linhas.Close()
	lista := []Conhecimento{}
	for linhas.Next() {
		var c Conhecimento
		if err := linhas.Scan(&c.ID, &c.WorkspaceID, &c.ProjetoID, &c.Titulo, &c.Texto, &c.Origem, &c.Estado,
			&c.Substitui, &c.AgenteID, &c.CriadaEm, &c.AtualizadaEm); err != nil {
			return nil, err
		}
		lista = append(lista, c)
	}
	return lista, linhas.Err()
}

// ListarConhecimento devolve as notas do workspace e dos projetos dele, as
// sugeridas também: as do workspace primeiro, na ordem em que nasceram.
func (b *Banco) ListarConhecimento(ctx context.Context, workspace int64) ([]Conhecimento, error) {
	if _, err := b.donoDoWorkspace(ctx, workspace); err != nil {
		return nil, err
	}
	linhas, err := b.db.QueryContext(ctx, `SELECT `+colunasConhecimento+deConhecimento+`
		WHERE c.workspace_id = ? OR p.workspace_id = ? ORDER BY c.projeto_id IS NOT NULL, c.projeto_id, c.id`, workspace, workspace)
	if err != nil {
		return nil, err
	}
	return lerConhecimento(linhas)
}

// ConhecimentoDoProjeto devolve as notas aprovadas que valem para o projeto:
// as do workspace dele e as dele, nessa ordem.
func (b *Banco) ConhecimentoDoProjeto(ctx context.Context, projeto int64) ([]Conhecimento, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT `+colunasConhecimento+deConhecimento+`
		WHERE c.estado = 'aprovada' AND (c.projeto_id = ? OR c.workspace_id = (SELECT workspace_id FROM projetos WHERE id = ?))
		ORDER BY c.projeto_id IS NOT NULL, c.id`, projeto, projeto)
	if err != nil {
		return nil, err
	}
	return lerConhecimento(linhas)
}

func (b *Banco) donoDoWorkspace(ctx context.Context, workspace int64) (int64, error) {
	var perfil int64
	err := b.db.QueryRowContext(ctx, `SELECT perfil_id FROM workspaces WHERE id = ?`, workspace).Scan(&perfil)
	if errors.Is(err, sql.ErrNoRows) {
		return 0, ErrNaoEncontrado
	}
	return perfil, err
}

// escopoDaNota diz de quem é a nota, para os eventos.
func escopoDaNota(ctx context.Context, tx *transacao, id int64) (Conhecimento, Escopo, error) {
	var c Conhecimento
	var perfil int64
	err := tx.QueryRowContext(ctx, `SELECT `+colunasConhecimento+`, w.perfil_id`+deConhecimento+`
		JOIN workspaces w ON w.id = COALESCE(c.workspace_id, p.workspace_id) WHERE c.id = ?`, id).Scan(
		&c.ID, &c.WorkspaceID, &c.ProjetoID, &c.Titulo, &c.Texto, &c.Origem, &c.Estado,
		&c.Substitui, &c.AgenteID, &c.CriadaEm, &c.AtualizadaEm, &perfil)
	if errors.Is(err, sql.ErrNoRows) {
		return c, Escopo{}, ErrNaoEncontrado
	}
	return c, Escopo{Perfil: perfil, Projeto: c.ProjetoID}, err
}

// O evento leva só os números e de onde veio a nota, nunca o texto: o
// histórico não se apaga, e uma senha colada por engano sai com a nota.
func eventoDaNota(c Conhecimento, perfil int64) map[string]any {
	return map[string]any{"perfil": perfil, "workspace": c.WorkspaceID, "projeto": c.ProjetoID, "id": c.ID,
		"origem": c.Origem, "estado": c.Estado}
}

// CriarConhecimento grava a nota. A sua nasce aprovada; a de um agente,
// sugerida, menos com a aprovação automática do workspace ligada.
func (b *Banco) CriarConhecimento(ctx context.Context, n NovoConhecimento) (Conhecimento, error) {
	titulo, texto, err := conhecimentoValido(n.Titulo, n.Texto)
	if err != nil {
		return Conhecimento{}, err
	}
	if err := umDe("origem", n.Origem, []string{OrigemVoce, OrigemAgente}); err != nil {
		return Conhecimento{}, err
	}
	var c Conhecimento
	err = b.emTransacao(ctx, func(tx *transacao) error {
		workspace := n.Workspace
		if n.Projeto != 0 {
			err := tx.QueryRowContext(ctx, `SELECT workspace_id FROM projetos WHERE id = ?`, n.Projeto).Scan(&workspace)
			if errors.Is(err, sql.ErrNoRows) {
				return ErrNaoEncontrado
			}
			if err != nil {
				return err
			}
			if n.Workspace != 0 && n.Workspace != workspace {
				return ErrInvalido{"o projeto não é deste workspace"}
			}
		}
		var auto bool
		err := tx.QueryRowContext(ctx, `SELECT conhecimento_auto FROM workspaces WHERE id = ?`, workspace).Scan(&auto)
		if errors.Is(err, sql.ErrNoRows) {
			return ErrNaoEncontrado
		}
		if err != nil {
			return err
		}
		estado := EstadoAprovada
		if n.Origem == OrigemAgente && !auto {
			estado = EstadoSugerida
			var pendentes int
			if err := tx.QueryRowContext(ctx, `SELECT COUNT(*)`+deConhecimento+` WHERE c.estado = 'sugerida' AND (c.workspace_id = ? OR p.workspace_id = ?)`,
				workspace, workspace).Scan(&pendentes); err != nil {
				return err
			}
			if pendentes >= MaxSugestoes {
				return ErrInvalido{"já há sugestões demais esperando o usuário na base de conhecimento; espere ele revisar"}
			}
		}
		if n.Substitui != 0 {
			alvo, _, err := escopoDaNota(ctx, tx, n.Substitui)
			if err != nil {
				return err
			}
			if alvo.Estado != EstadoAprovada || alvo.WorkspaceID != workspace || alvo.ProjetoID != n.Projeto {
				return ErrInvalido{"a nota a mudar não é deste lugar da base de conhecimento"}
			}
		}
		// Aprovada na hora, a mudança vai direto para a nota.
		if n.Substitui != 0 && estado == EstadoAprovada {
			if _, err := tx.ExecContext(ctx, `UPDATE conhecimento SET titulo = ?, texto = ?, atualizada_em = ? WHERE id = ?`,
				titulo, texto, agora(), n.Substitui); err != nil {
				return err
			}
			var escopo Escopo
			c, escopo, err = escopoDaNota(ctx, tx, n.Substitui)
			if err != nil {
				return err
			}
			return registrar(ctx, tx, "conhecimento.editado", escopo, eventoDaNota(c, escopo.Perfil))
		}
		momento := agora()
		var noWorkspace any = workspace
		if n.Projeto != 0 {
			noWorkspace = nil
		}
		r, err := tx.ExecContext(ctx, `INSERT INTO conhecimento (workspace_id, projeto_id, titulo, texto, origem, estado, substitui, agente_id, criada_em, atualizada_em)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`, noWorkspace, nulo(n.Projeto), titulo, texto, n.Origem, estado, nulo(n.Substitui), n.Agente, momento, momento)
		if err != nil {
			return err
		}
		id, _ := r.LastInsertId()
		var escopo Escopo
		c, escopo, err = escopoDaNota(ctx, tx, id)
		if err != nil {
			return err
		}
		tipo := "conhecimento.criado"
		if estado == EstadoSugerida {
			tipo = "conhecimento.sugerido"
		}
		return registrar(ctx, tx, tipo, escopo, eventoDaNota(c, escopo.Perfil))
	})
	return c, err
}

// EditarConhecimento troca o título e o texto. Editar uma sugestão é aceitá-la
// com as suas mudanças.
func (b *Banco) EditarConhecimento(ctx context.Context, id int64, titulo, texto string) (Conhecimento, error) {
	titulo, texto, err := conhecimentoValido(titulo, texto)
	if err != nil {
		return Conhecimento{}, err
	}
	var c Conhecimento
	err = b.emTransacao(ctx, func(tx *transacao) error {
		atual, _, err := escopoDaNota(ctx, tx, id)
		if err != nil {
			return err
		}
		if atual.Estado == EstadoSugerida {
			c, err = aprovar(ctx, tx, atual, titulo, texto)
			return err
		}
		if _, err := tx.ExecContext(ctx, `UPDATE conhecimento SET titulo = ?, texto = ?, atualizada_em = ? WHERE id = ?`, titulo, texto, agora(), id); err != nil {
			return err
		}
		var escopo Escopo
		c, escopo, err = escopoDaNota(ctx, tx, id)
		if err != nil {
			return err
		}
		return registrar(ctx, tx, "conhecimento.editado", escopo, eventoDaNota(c, escopo.Perfil))
	})
	return c, err
}

// AprovarConhecimento aceita a sugestão como veio.
func (b *Banco) AprovarConhecimento(ctx context.Context, id int64) (Conhecimento, error) {
	var c Conhecimento
	err := b.emTransacao(ctx, func(tx *transacao) error {
		atual, _, err := escopoDaNota(ctx, tx, id)
		if err != nil {
			return err
		}
		if atual.Estado != EstadoSugerida {
			c = atual
			return nil
		}
		c, err = aprovar(ctx, tx, atual, atual.Titulo, atual.Texto)
		return err
	})
	return c, err
}

// aprovar aceita a sugestão: a que muda uma nota passa o texto para ela e
// some; a nova fica aprovada.
func aprovar(ctx context.Context, tx *transacao, s Conhecimento, titulo, texto string) (Conhecimento, error) {
	alvo := s.ID
	if s.Substitui != 0 {
		alvo = s.Substitui
		if _, err := tx.ExecContext(ctx, `DELETE FROM conhecimento WHERE id = ?`, s.ID); err != nil {
			return Conhecimento{}, err
		}
	}
	if _, err := tx.ExecContext(ctx, `UPDATE conhecimento SET titulo = ?, texto = ?, estado = 'aprovada', atualizada_em = ? WHERE id = ?`,
		titulo, texto, agora(), alvo); err != nil {
		return Conhecimento{}, err
	}
	c, escopo, err := escopoDaNota(ctx, tx, alvo)
	if err != nil {
		return c, err
	}
	return c, registrar(ctx, tx, "conhecimento.aprovado", escopo, eventoDaNota(c, escopo.Perfil))
}

// RemoverConhecimento apaga a nota (ou recusa a sugestão), com as sugestões
// que queriam mudá-la.
func (b *Banco) RemoverConhecimento(ctx context.Context, id int64) error {
	return b.emTransacao(ctx, func(tx *transacao) error {
		c, escopo, err := escopoDaNota(ctx, tx, id)
		if err != nil {
			return err
		}
		if _, err := tx.ExecContext(ctx, `DELETE FROM conhecimento WHERE id = ?`, id); err != nil {
			return err
		}
		return registrar(ctx, tx, "conhecimento.removido", escopo, eventoDaNota(c, escopo.Perfil))
	})
}

// PerfilDoConhecimento diz de que perfil é a nota.
func (b *Banco) PerfilDoConhecimento(ctx context.Context, id int64) (int64, error) {
	var perfil int64
	err := b.db.QueryRowContext(ctx, `SELECT w.perfil_id`+deConhecimento+`
		JOIN workspaces w ON w.id = COALESCE(c.workspace_id, p.workspace_id) WHERE c.id = ?`, id).Scan(&perfil)
	if errors.Is(err, sql.ErrNoRows) {
		return 0, ErrNaoEncontrado
	}
	return perfil, err
}

// TextoDoConhecimento monta o que o agente recebe ao começar, dentro de
// MaxConhecimentoInicio caracteres. Sem notas, devolve "" (nada vai ao agente).
func TextoDoConhecimento(notas []Conhecimento, workspace, projeto string) string {
	if len(notas) == 0 {
		return ""
	}
	var t strings.Builder
	fmt.Fprintf(&t, "# Base de conhecimento da Colmeia\n\nO que o usuário deixou anotado para todo agente do workspace %q e do projeto %q. Siga sem ele precisar repetir.\n", workspace, projeto)
	var fora []string
	for _, n := range notas {
		onde := "workspace"
		if n.ProjetoID != 0 {
			onde = "projeto"
		}
		bloco := fmt.Sprintf("\n## %s (%s, nota %d)\n\n%s\n", n.Titulo, onde, n.ID, n.Texto)
		if len(fora) > 0 || utf8.RuneCountInString(t.String())+utf8.RuneCountInString(bloco) > MaxConhecimentoInicio {
			fora = append(fora, fmt.Sprintf("%q (nota %d)", n.Titulo, n.ID))
			continue
		}
		t.WriteString(bloco)
	}
	if len(fora) > 0 {
		fmt.Fprintf(&t, "\nOutras notas, que não couberam aqui: %s. Leia com ler_conhecimento quando precisar.\n", strings.Join(fora, ", "))
	}
	t.WriteString("\nSe descobrir algo que valeria para as próximas tarefas deste projeto ou workspace, proponha com anotar_conhecimento, curto. O usuário aprova antes de valer.\n")
	return t.String()
}

// SugestoesPorWorkspace conta as sugestões dos agentes esperando você em cada
// workspace do perfil (só os que têm alguma).
func (b *Banco) SugestoesPorWorkspace(ctx context.Context, perfil int64) (map[int64]int, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT w.id, COUNT(*)`+deConhecimento+`
		JOIN workspaces w ON w.id = COALESCE(c.workspace_id, p.workspace_id)
		WHERE w.perfil_id = ? AND c.estado = 'sugerida' GROUP BY w.id`, perfil)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	contagem := map[int64]int{}
	for linhas.Next() {
		var w int64
		var n int
		if err := linhas.Scan(&w, &n); err != nil {
			return nil, err
		}
		contagem[w] = n
	}
	return contagem, linhas.Err()
}
