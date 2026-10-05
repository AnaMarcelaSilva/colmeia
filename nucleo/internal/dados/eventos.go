package dados

import (
	"context"
	"encoding/json"
	"strings"
)

// FiltroEventos escolhe uma fatia do histórico de um perfil.
type FiltroEventos struct {
	Perfil  int64
	Projeto int64 // 0: todos os projetos do perfil
	// Workspace: só os projetos desse workspace (0: não filtra). Uma consulta
	// só, com os projetos lidos numa subconsulta, nunca uma por projeto.
	Workspace int64
	// Antes pagina de trás para frente: só eventos com id menor (0: sem limite).
	Antes int64
	// Desde e Ate limitam o momento (RFC 3339, UTC); vazio não limita.
	Desde, Ate string
	// Tipos limita aos tipos dados; vazio traz todos.
	Tipos []string
	// Limite de eventos (0: sem limite). A ordem é do mais novo para o mais antigo.
	Limite int
}

// ListarEventos traz os eventos do perfil, do mais novo para o mais antigo.
func (b *Banco) ListarEventos(ctx context.Context, f FiltroEventos) ([]Evento, error) {
	consulta := `SELECT id, momento, tipo, dados, COALESCE(perfil_id, 0), COALESCE(projeto_id, 0), COALESCE(tarefa_id, 0), COALESCE(agente_id, 0)
		FROM eventos WHERE perfil_id = ?`
	args := []any{f.Perfil}
	if f.Projeto != 0 {
		consulta += ` AND projeto_id = ?`
		args = append(args, f.Projeto)
	}
	if f.Workspace != 0 {
		consulta += ` AND projeto_id IN (SELECT id FROM projetos WHERE workspace_id = ?)`
		args = append(args, f.Workspace)
	}
	if f.Antes > 0 {
		consulta += ` AND id < ?`
		args = append(args, f.Antes)
	}
	if f.Desde != "" {
		consulta += ` AND momento >= ?`
		args = append(args, f.Desde)
	}
	if f.Ate != "" {
		consulta += ` AND momento < ?`
		args = append(args, f.Ate)
	}
	if len(f.Tipos) > 0 {
		consulta += ` AND tipo IN (?` + strings.Repeat(`, ?`, len(f.Tipos)-1) + `)`
		for _, t := range f.Tipos {
			args = append(args, t)
		}
	}
	consulta += ` ORDER BY id DESC`
	if f.Limite > 0 {
		consulta += ` LIMIT ?`
		args = append(args, f.Limite)
	}
	linhas, err := b.db.QueryContext(ctx, consulta, args...)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []Evento{}
	for linhas.Next() {
		var e Evento
		var conteudo string
		if err := linhas.Scan(&e.ID, &e.Momento, &e.Tipo, &conteudo, &e.Escopo.Perfil, &e.Escopo.Projeto, &e.Escopo.Tarefa, &e.Escopo.Agente); err != nil {
			return nil, err
		}
		e.Dados = json.RawMessage(conteudo)
		lista = append(lista, e)
	}
	return lista, linhas.Err()
}

// UltimosFins traz, por agente do perfil, o último agente.terminou gravado:
// ao abrir a tela, um erro ou o fim de um agente parado aparecem sem esperar
// a próxima mudança.
func (b *Banco) UltimosFins(ctx context.Context, perfil int64) (map[int64]Evento, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT id, momento, tipo, dados, agente_id FROM eventos
		WHERE id IN (SELECT MAX(id) FROM eventos WHERE perfil_id = ? AND tipo = 'agente.terminou' GROUP BY agente_id)`, perfil)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	fins := map[int64]Evento{}
	for linhas.Next() {
		var e Evento
		var conteudo string
		if err := linhas.Scan(&e.ID, &e.Momento, &e.Tipo, &conteudo, &e.Escopo.Agente); err != nil {
			return nil, err
		}
		e.Dados = json.RawMessage(conteudo)
		fins[e.Escopo.Agente] = e
	}
	return fins, linhas.Err()
}
