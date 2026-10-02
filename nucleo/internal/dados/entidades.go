package dados

import (
	"context"
	"database/sql"
	"errors"
	"strings"
)

type Perfil struct {
	ID       int64  `json:"id"`
	Nome     string `json:"nome"`
	Tema     string `json:"tema"`
	CriadoEm string `json:"criado_em"`
}

type Conta struct {
	Ferramenta string `json:"ferramenta"`
	Modo       string `json:"modo"`
}

type Workspace struct {
	ID       int64  `json:"id"`
	PerfilID int64  `json:"perfil_id"`
	Nome     string `json:"nome"`
}

type Projeto struct {
	ID          int64  `json:"id"`
	WorkspaceID int64  `json:"workspace_id"`
	Workspace   string `json:"workspace"`
	Nome        string `json:"nome"`
	Caminho     string `json:"caminho"`
	// Tipo é "git" (repositório) ou "pasta" (pasta de trabalho sem git, sem branches).
	Tipo         string `json:"tipo"`
	BranchPadrao string `json:"branch_padrao"`
}

type Tarefa struct {
	ID        int64  `json:"id"`
	ProjetoID int64  `json:"projeto_id"`
	Titulo    string `json:"titulo"`
	Coluna    string `json:"coluna"`
	Branch    string `json:"branch"`
	// Local é onde os agentes trabalham: "pasta" (a do projeto) ou "copia"
	// (uma cópia isolada, com a branch da tarefa, no caminho Copia).
	Local        string  `json:"local"`
	Copia        string  `json:"copia"`
	Ordem        float64 `json:"ordem"`
	CriadoEm     string  `json:"criado_em"`
	AtualizadoEm string  `json:"atualizado_em"`
}

// Perfis

func (b *Banco) ListarPerfis(ctx context.Context) ([]Perfil, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT id, nome, tema, criado_em FROM perfis ORDER BY nome`)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	perfis := []Perfil{}
	for linhas.Next() {
		var p Perfil
		if err := linhas.Scan(&p.ID, &p.Nome, &p.Tema, &p.CriadoEm); err != nil {
			return nil, err
		}
		perfis = append(perfis, p)
	}
	return perfis, linhas.Err()
}

func (b *Banco) CriarPerfil(ctx context.Context, nome, tema string) (Perfil, error) {
	nome, err := nomeValido("O nome do perfil", nome, 60)
	if err != nil {
		return Perfil{}, err
	}
	if tema == "" {
		tema = "escuro"
	}
	if err := umDe("tema", tema, Temas); err != nil {
		return Perfil{}, err
	}
	p := Perfil{Nome: nome, Tema: tema, CriadoEm: agora()}
	err = b.emTransacao(ctx, func(tx *sql.Tx) error {
		r, err := tx.ExecContext(ctx, `INSERT INTO perfis (nome, tema, criado_em) VALUES (?, ?, ?)`, p.Nome, p.Tema, p.CriadoEm)
		if err != nil {
			return traduzir(err)
		}
		p.ID, _ = r.LastInsertId()
		return registrar(ctx, tx, "perfil.criado", p)
	})
	return p, err
}

func (b *Banco) DefinirTema(ctx context.Context, perfil int64, tema string) error {
	if err := umDe("tema", tema, Temas); err != nil {
		return err
	}
	return b.emTransacao(ctx, func(tx *sql.Tx) error {
		r, err := tx.ExecContext(ctx, `UPDATE perfis SET tema = ? WHERE id = ?`, tema, perfil)
		if err != nil {
			return err
		}
		if n, _ := r.RowsAffected(); n == 0 {
			return ErrNaoEncontrado
		}
		return registrar(ctx, tx, "perfil.tema", map[string]any{"perfil": perfil, "tema": tema})
	})
}

func (b *Banco) existePerfil(ctx context.Context, perfil int64) error {
	var um int
	err := b.db.QueryRowContext(ctx, `SELECT 1 FROM perfis WHERE id = ?`, perfil).Scan(&um)
	if errors.Is(err, sql.ErrNoRows) {
		return ErrNaoEncontrado
	}
	return err
}

// Contas de IA

func (b *Banco) ListarContas(ctx context.Context, perfil int64) ([]Conta, error) {
	if err := b.existePerfil(ctx, perfil); err != nil {
		return nil, err
	}
	linhas, err := b.db.QueryContext(ctx, `SELECT ferramenta, modo FROM contas_ia WHERE perfil_id = ? ORDER BY ferramenta`, perfil)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	contas := []Conta{}
	for linhas.Next() {
		var c Conta
		if err := linhas.Scan(&c.Ferramenta, &c.Modo); err != nil {
			return nil, err
		}
		contas = append(contas, c)
	}
	return contas, linhas.Err()
}

// DefinirContas troca a lista inteira de ferramentas do perfil.
func (b *Banco) DefinirContas(ctx context.Context, perfil int64, contas []Conta) error {
	if err := b.existePerfil(ctx, perfil); err != nil {
		return err
	}
	vistas := map[string]bool{}
	for _, c := range contas {
		if err := umDe("ferramenta", c.Ferramenta, Ferramentas); err != nil {
			return err
		}
		if err := umDe("modo da conta", c.Modo, ModosConta); err != nil {
			return err
		}
		if vistas[c.Ferramenta] {
			return ErrInvalido{"ferramenta repetida: " + c.Ferramenta}
		}
		vistas[c.Ferramenta] = true
	}
	return b.emTransacao(ctx, func(tx *sql.Tx) error {
		if _, err := tx.ExecContext(ctx, `DELETE FROM contas_ia WHERE perfil_id = ?`, perfil); err != nil {
			return err
		}
		for _, c := range contas {
			if _, err := tx.ExecContext(ctx, `INSERT INTO contas_ia (perfil_id, ferramenta, modo) VALUES (?, ?, ?)`, perfil, c.Ferramenta, c.Modo); err != nil {
				return err
			}
		}
		return registrar(ctx, tx, "perfil.contas", map[string]any{"perfil": perfil, "contas": contas})
	})
}

// Workspaces

func (b *Banco) ListarWorkspaces(ctx context.Context, perfil int64) ([]Workspace, error) {
	if err := b.existePerfil(ctx, perfil); err != nil {
		return nil, err
	}
	linhas, err := b.db.QueryContext(ctx, `SELECT id, perfil_id, nome FROM workspaces WHERE perfil_id = ? ORDER BY nome`, perfil)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []Workspace{}
	for linhas.Next() {
		var w Workspace
		if err := linhas.Scan(&w.ID, &w.PerfilID, &w.Nome); err != nil {
			return nil, err
		}
		lista = append(lista, w)
	}
	return lista, linhas.Err()
}

func (b *Banco) CriarWorkspace(ctx context.Context, perfil int64, nome string) (Workspace, error) {
	nome, err := nomeValido("O nome do workspace", nome, 60)
	if err != nil {
		return Workspace{}, err
	}
	if err := b.existePerfil(ctx, perfil); err != nil {
		return Workspace{}, err
	}
	w := Workspace{PerfilID: perfil, Nome: nome}
	err = b.emTransacao(ctx, func(tx *sql.Tx) error {
		r, err := tx.ExecContext(ctx, `INSERT INTO workspaces (perfil_id, nome) VALUES (?, ?)`, perfil, nome)
		if err != nil {
			return traduzir(err)
		}
		w.ID, _ = r.LastInsertId()
		return registrar(ctx, tx, "workspace.criado", w)
	})
	return w, err
}

// Projetos

func (b *Banco) ListarProjetos(ctx context.Context, perfil int64) ([]Projeto, error) {
	if err := b.existePerfil(ctx, perfil); err != nil {
		return nil, err
	}
	linhas, err := b.db.QueryContext(ctx, `
		SELECT p.id, p.workspace_id, w.nome, p.nome, p.caminho, p.tipo, p.branch_padrao
		FROM projetos p JOIN workspaces w ON w.id = p.workspace_id
		WHERE w.perfil_id = ? ORDER BY w.nome, p.nome`, perfil)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []Projeto{}
	for linhas.Next() {
		var p Projeto
		if err := linhas.Scan(&p.ID, &p.WorkspaceID, &p.Workspace, &p.Nome, &p.Caminho, &p.Tipo, &p.BranchPadrao); err != nil {
			return nil, err
		}
		lista = append(lista, p)
	}
	return lista, linhas.Err()
}

func (b *Banco) Projeto(ctx context.Context, id int64) (Projeto, error) {
	var p Projeto
	err := b.db.QueryRowContext(ctx, `
		SELECT p.id, p.workspace_id, w.nome, p.nome, p.caminho, p.tipo, p.branch_padrao
		FROM projetos p JOIN workspaces w ON w.id = p.workspace_id WHERE p.id = ?`, id).
		Scan(&p.ID, &p.WorkspaceID, &p.Workspace, &p.Nome, &p.Caminho, &p.Tipo, &p.BranchPadrao)
	if errors.Is(err, sql.ErrNoRows) {
		return p, ErrNaoEncontrado
	}
	return p, err
}

// CriarProjeto grava um projeto já validado (a API confere a pasta e se ela
// é um repositório git). Uma pasta sem git não tem branch padrão.
func (b *Banco) CriarProjeto(ctx context.Context, workspace int64, nome, caminho, tipo, branchPadrao string) (Projeto, error) {
	nome, err := nomeValido("O nome do projeto", nome, 80)
	if err != nil {
		return Projeto{}, err
	}
	if err := umDe("tipo de projeto", tipo, TiposProjeto); err != nil {
		return Projeto{}, err
	}
	if tipo == "git" {
		if err := BranchValida(branchPadrao); err != nil {
			return Projeto{}, err
		}
	} else {
		branchPadrao = ""
	}
	p := Projeto{WorkspaceID: workspace, Nome: nome, Caminho: caminho, Tipo: tipo, BranchPadrao: branchPadrao}
	err = b.emTransacao(ctx, func(tx *sql.Tx) error {
		if err := tx.QueryRowContext(ctx, `SELECT nome FROM workspaces WHERE id = ?`, workspace).Scan(&p.Workspace); err != nil {
			if errors.Is(err, sql.ErrNoRows) {
				return ErrNaoEncontrado
			}
			return err
		}
		r, err := tx.ExecContext(ctx, `INSERT INTO projetos (workspace_id, nome, caminho, tipo, branch_padrao) VALUES (?, ?, ?, ?, ?)`, workspace, nome, caminho, tipo, branchPadrao)
		if err != nil {
			return traduzir(err)
		}
		p.ID, _ = r.LastInsertId()
		return registrar(ctx, tx, "projeto.criado", p)
	})
	return p, err
}

// RemoverProjeto tira o projeto (e suas tarefas) da Colmeia. A pasta não é tocada.
func (b *Banco) RemoverProjeto(ctx context.Context, id int64) error {
	return b.emTransacao(ctx, func(tx *sql.Tx) error {
		r, err := tx.ExecContext(ctx, `DELETE FROM projetos WHERE id = ?`, id)
		if err != nil {
			return err
		}
		if n, _ := r.RowsAffected(); n == 0 {
			return ErrNaoEncontrado
		}
		return registrar(ctx, tx, "projeto.removido", map[string]any{"projeto": id})
	})
}

// Tarefas

func (b *Banco) ListarTarefas(ctx context.Context, projeto int64) ([]Tarefa, error) {
	if _, err := b.Projeto(ctx, projeto); err != nil {
		return nil, err
	}
	linhas, err := b.db.QueryContext(ctx, `
		SELECT id, projeto_id, titulo, coluna, branch, local, copia, ordem, criado_em, atualizado_em
		FROM tarefas WHERE projeto_id = ? ORDER BY coluna, ordem`, projeto)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []Tarefa{}
	for linhas.Next() {
		var t Tarefa
		if err := linhas.Scan(&t.ID, &t.ProjetoID, &t.Titulo, &t.Coluna, &t.Branch, &t.Local, &t.Copia, &t.Ordem, &t.CriadoEm, &t.AtualizadoEm); err != nil {
			return nil, err
		}
		lista = append(lista, t)
	}
	return lista, linhas.Err()
}

// CriarTarefa grava a tarefa. Numa pasta sem git ela não tem branch e os
// agentes trabalham direto na pasta. A cópia isolada é criada pela API, que
// depois grava o caminho com DefinirCopia.
func (b *Banco) CriarTarefa(ctx context.Context, projeto int64, titulo, branch, local string) (Tarefa, error) {
	titulo, err := nomeValido("O título da tarefa", titulo, 200)
	if err != nil {
		return Tarefa{}, err
	}
	p, err := b.Projeto(ctx, projeto)
	if err != nil {
		return Tarefa{}, err
	}
	if local == "" {
		local = "pasta"
	}
	if err := umDe("local da tarefa", local, Locais); err != nil {
		return Tarefa{}, err
	}
	if p.Tipo == "pasta" {
		if branch != "" || local != "pasta" {
			return Tarefa{}, ErrInvalido{"uma pasta sem git não tem branches nem cópias isoladas"}
		}
	} else if err := BranchValida(branch); err != nil {
		return Tarefa{}, err
	}
	momento := agora()
	t := Tarefa{ProjetoID: projeto, Titulo: titulo, Coluna: "backlog", Branch: branch, Local: local, CriadoEm: momento, AtualizadoEm: momento}
	err = b.emTransacao(ctx, func(tx *sql.Tx) error {
		// A tarefa nova vai para o fim do Backlog.
		if err := tx.QueryRowContext(ctx, `SELECT COALESCE(MAX(ordem), 0) + 1 FROM tarefas WHERE projeto_id = ? AND coluna = 'backlog'`, projeto).Scan(&t.Ordem); err != nil {
			return err
		}
		r, err := tx.ExecContext(ctx, `INSERT INTO tarefas (projeto_id, titulo, coluna, branch, local, ordem, criado_em, atualizado_em) VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
			t.ProjetoID, t.Titulo, t.Coluna, t.Branch, t.Local, t.Ordem, t.CriadoEm, t.AtualizadoEm)
		if err != nil {
			return err
		}
		t.ID, _ = r.LastInsertId()
		return registrar(ctx, tx, "tarefa.criada", t)
	})
	return t, err
}

// Mudanca traz só os campos que mudam; nil fica como está.
type Mudanca struct {
	Titulo *string `json:"titulo"`
	Coluna *string `json:"coluna"`
	Branch *string `json:"branch"`
}

func (b *Banco) AtualizarTarefa(ctx context.Context, id int64, m Mudanca) (Tarefa, error) {
	if m.Titulo != nil {
		titulo, err := nomeValido("O título da tarefa", *m.Titulo, 200)
		if err != nil {
			return Tarefa{}, err
		}
		m.Titulo = &titulo
	}
	if m.Coluna != nil {
		if err := umDe("coluna", *m.Coluna, Colunas); err != nil {
			return Tarefa{}, err
		}
	}
	if m.Branch != nil && *m.Branch != "" {
		if err := BranchValida(*m.Branch); err != nil {
			return Tarefa{}, err
		}
	}
	var t Tarefa
	err := b.emTransacao(ctx, func(tx *sql.Tx) error {
		err := tx.QueryRowContext(ctx, `SELECT id, projeto_id, titulo, coluna, branch, local, copia, ordem, criado_em, atualizado_em FROM tarefas WHERE id = ?`, id).
			Scan(&t.ID, &t.ProjetoID, &t.Titulo, &t.Coluna, &t.Branch, &t.Local, &t.Copia, &t.Ordem, &t.CriadoEm, &t.AtualizadoEm)
		if errors.Is(err, sql.ErrNoRows) {
			return ErrNaoEncontrado
		}
		if err != nil {
			return err
		}
		if m.Titulo != nil {
			t.Titulo = *m.Titulo
		}
		if m.Branch != nil && *m.Branch != t.Branch {
			// A branch de uma cópia isolada é a da cópia; a de uma pasta sem git não existe.
			var tipo string
			if err := tx.QueryRowContext(ctx, `SELECT tipo FROM projetos WHERE id = ?`, t.ProjetoID).Scan(&tipo); err != nil {
				return err
			}
			if t.Local == "copia" || (tipo == "pasta") != (*m.Branch == "") {
				return ErrInvalido{"a branch desta tarefa não pode ser trocada"}
			}
			t.Branch = *m.Branch
		}
		if m.Coluna != nil && *m.Coluna != t.Coluna {
			// Mudou de coluna: vai para o fim da coluna de destino.
			t.Coluna = *m.Coluna
			if err := tx.QueryRowContext(ctx, `SELECT COALESCE(MAX(ordem), 0) + 1 FROM tarefas WHERE projeto_id = ? AND coluna = ?`, t.ProjetoID, t.Coluna).Scan(&t.Ordem); err != nil {
				return err
			}
		}
		t.AtualizadoEm = agora()
		if _, err := tx.ExecContext(ctx, `UPDATE tarefas SET titulo = ?, coluna = ?, branch = ?, ordem = ?, atualizado_em = ? WHERE id = ?`,
			t.Titulo, t.Coluna, t.Branch, t.Ordem, t.AtualizadoEm, t.ID); err != nil {
			return err
		}
		return registrar(ctx, tx, "tarefa.atualizada", map[string]any{"tarefa": t.ID, "mudanca": m})
	})
	return t, err
}

// Tarefa devolve a tarefa com o projeto dela e o perfil a que pertence.
func (b *Banco) Tarefa(ctx context.Context, id int64) (Tarefa, Projeto, int64, error) {
	var t Tarefa
	err := b.db.QueryRowContext(ctx, `SELECT id, projeto_id, titulo, coluna, branch, local, copia, ordem, criado_em, atualizado_em FROM tarefas WHERE id = ?`, id).
		Scan(&t.ID, &t.ProjetoID, &t.Titulo, &t.Coluna, &t.Branch, &t.Local, &t.Copia, &t.Ordem, &t.CriadoEm, &t.AtualizadoEm)
	if errors.Is(err, sql.ErrNoRows) {
		return t, Projeto{}, 0, ErrNaoEncontrado
	}
	if err != nil {
		return t, Projeto{}, 0, err
	}
	p, err := b.Projeto(ctx, t.ProjetoID)
	if err != nil {
		return t, p, 0, err
	}
	var perfil int64
	err = b.db.QueryRowContext(ctx, `SELECT perfil_id FROM workspaces WHERE id = ?`, p.WorkspaceID).Scan(&perfil)
	return t, p, perfil, err
}

// DefinirCopia grava onde ficou a cópia isolada da tarefa.
func (b *Banco) DefinirCopia(ctx context.Context, id int64, caminho string) error {
	return b.emTransacao(ctx, func(tx *sql.Tx) error {
		if _, err := tx.ExecContext(ctx, `UPDATE tarefas SET copia = ? WHERE id = ?`, caminho, id); err != nil {
			return err
		}
		return registrar(ctx, tx, "tarefa.copia", map[string]any{"tarefa": id, "copia": caminho})
	})
}

// Pasta é onde os agentes da tarefa trabalham.
func (t Tarefa) Pasta(p Projeto) string {
	if t.Local == "copia" && t.Copia != "" {
		return t.Copia
	}
	return p.Caminho
}

func (b *Banco) RemoverTarefa(ctx context.Context, id int64) error {
	return b.emTransacao(ctx, func(tx *sql.Tx) error {
		r, err := tx.ExecContext(ctx, `DELETE FROM tarefas WHERE id = ?`, id)
		if err != nil {
			return err
		}
		if n, _ := r.RowsAffected(); n == 0 {
			return ErrNaoEncontrado
		}
		return registrar(ctx, tx, "tarefa.removida", map[string]any{"tarefa": id})
	})
}

// Agentes

type Agente struct {
	ID         int64  `json:"id"`
	TarefaID   int64  `json:"tarefa_id"`
	Ferramenta string `json:"ferramenta"`
	Papel      string `json:"papel"`
	// Sessao é a conversa do Claude Code que o agente retomou, se houver.
	Sessao   string `json:"sessao"`
	CriadoEm string `json:"criado_em"`
}

func (b *Banco) ListarAgentes(ctx context.Context, tarefa int64) ([]Agente, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT id, tarefa_id, ferramenta, papel, sessao, criado_em FROM agentes WHERE tarefa_id = ? ORDER BY id`, tarefa)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []Agente{}
	for linhas.Next() {
		var a Agente
		if err := linhas.Scan(&a.ID, &a.TarefaID, &a.Ferramenta, &a.Papel, &a.Sessao, &a.CriadoEm); err != nil {
			return nil, err
		}
		lista = append(lista, a)
	}
	return lista, linhas.Err()
}

// ListarAgentesDoProjeto traz os agentes de todas as tarefas do projeto de uma vez.
func (b *Banco) ListarAgentesDoProjeto(ctx context.Context, projeto int64) ([]Agente, error) {
	if _, err := b.Projeto(ctx, projeto); err != nil {
		return nil, err
	}
	linhas, err := b.db.QueryContext(ctx, `
		SELECT a.id, a.tarefa_id, a.ferramenta, a.papel, a.sessao, a.criado_em
		FROM agentes a JOIN tarefas t ON t.id = a.tarefa_id WHERE t.projeto_id = ? ORDER BY a.id`, projeto)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []Agente{}
	for linhas.Next() {
		var a Agente
		if err := linhas.Scan(&a.ID, &a.TarefaID, &a.Ferramenta, &a.Papel, &a.Sessao, &a.CriadoEm); err != nil {
			return nil, err
		}
		lista = append(lista, a)
	}
	return lista, linhas.Err()
}

func (b *Banco) Agente(ctx context.Context, id int64) (Agente, error) {
	var a Agente
	err := b.db.QueryRowContext(ctx, `SELECT id, tarefa_id, ferramenta, papel, sessao, criado_em FROM agentes WHERE id = ?`, id).
		Scan(&a.ID, &a.TarefaID, &a.Ferramenta, &a.Papel, &a.Sessao, &a.CriadoEm)
	if errors.Is(err, sql.ErrNoRows) {
		return a, ErrNaoEncontrado
	}
	return a, err
}

// CriarAgente grava o agente; a sessão (já validada pela API) só vale para o Claude Code.
func (b *Banco) CriarAgente(ctx context.Context, tarefa int64, ferramenta, papel, sessao string) (Agente, error) {
	if err := umDe("ferramenta", ferramenta, TiposAgente); err != nil {
		return Agente{}, err
	}
	if err := umDe("papel", papel, Papeis); err != nil {
		return Agente{}, err
	}
	if sessao != "" && ferramenta != "claude" {
		return Agente{}, ErrInvalido{"só o Claude Code retoma uma conversa"}
	}
	a := Agente{TarefaID: tarefa, Ferramenta: ferramenta, Papel: papel, Sessao: sessao, CriadoEm: agora()}
	err := b.emTransacao(ctx, func(tx *sql.Tx) error {
		r, err := tx.ExecContext(ctx, `INSERT INTO agentes (tarefa_id, ferramenta, papel, sessao, criado_em) VALUES (?, ?, ?, ?, ?)`,
			a.TarefaID, a.Ferramenta, a.Papel, a.Sessao, a.CriadoEm)
		if err != nil {
			if strings.Contains(err.Error(), "FOREIGN KEY") {
				return ErrNaoEncontrado
			}
			return err
		}
		a.ID, _ = r.LastInsertId()
		return registrar(ctx, tx, "agente.criado", a)
	})
	return a, err
}

func (b *Banco) RemoverAgente(ctx context.Context, id int64) error {
	return b.emTransacao(ctx, func(tx *sql.Tx) error {
		r, err := tx.ExecContext(ctx, `DELETE FROM agentes WHERE id = ?`, id)
		if err != nil {
			return err
		}
		if n, _ := r.RowsAffected(); n == 0 {
			return ErrNaoEncontrado
		}
		return registrar(ctx, tx, "agente.removido", map[string]any{"agente": id})
	})
}
