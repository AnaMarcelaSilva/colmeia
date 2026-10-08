package dados

import (
	"context"
	"database/sql"
	"errors"
	"strings"
	"time"
)

// MaxDiasSprint é o período mais longo de uma sprint (e da página da sprint).
const MaxDiasSprint = 92

// MaxTituloSprint em caracteres: o título que a tarefa ganha numa sprint.
const MaxTituloSprint = 200

// Sprint é um período fixo do perfil (as datas no fuso local, inclusive).
// Fixo porque o que você escreve para a reunião (os títulos e as notas da
// sprint) fica guardado por ele: "os últimos 14 dias" muda todo dia.
type Sprint struct {
	ID     int64  `json:"id"`
	Perfil int64  `json:"perfil_id"`
	Inicio string `json:"inicio"`
	Fim    string `json:"fim"`
}

// ChaveNota é o período das notas desta sprint.
func (s Sprint) ChaveNota() string { return s.Inicio + ".." + s.Fim }

// Dias da sprint, contando o início e o fim.
func (s Sprint) Dias() int {
	de, _ := time.Parse("2006-01-02", s.Inicio)
	ate, _ := time.Parse("2006-01-02", s.Fim)
	return int(ate.Sub(de).Hours()/24) + 1
}

func periodoSprintValido(inicio, fim string) error {
	if !padraoDia.MatchString(inicio) || !dataReal(inicio) || !padraoDia.MatchString(fim) || !dataReal(fim) {
		return ErrInvalido{"as datas da sprint precisam estar no formato AAAA-MM-DD"}
	}
	if fim < inicio {
		return ErrInvalido{"a sprint termina antes de começar"}
	}
	if (Sprint{Inicio: inicio, Fim: fim}).Dias() > MaxDiasSprint {
		return ErrInvalido{"a sprint pode ter no máximo 92 dias"}
	}
	return nil
}

func lerSprint(linha interface{ Scan(...any) error }) (Sprint, error) {
	var s Sprint
	err := linha.Scan(&s.ID, &s.Perfil, &s.Inicio, &s.Fim)
	if errors.Is(err, sql.ErrNoRows) {
		return s, ErrNaoEncontrado
	}
	return s, err
}

// ListarSprints traz as sprints do perfil, da mais antiga para a mais nova.
func (b *Banco) ListarSprints(ctx context.Context, perfil int64) ([]Sprint, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT id, perfil_id, inicio, fim FROM sprints WHERE perfil_id = ? ORDER BY inicio`, perfil)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []Sprint{}
	for linhas.Next() {
		s, err := lerSprint(linhas)
		if err != nil {
			return nil, err
		}
		lista = append(lista, s)
	}
	return lista, linhas.Err()
}

func (b *Banco) SprintPorID(ctx context.Context, id int64) (Sprint, error) {
	return lerSprint(b.db.QueryRowContext(ctx, `SELECT id, perfil_id, inicio, fim FROM sprints WHERE id = ?`, id))
}

// sobrepoe diz se o período cruza outra sprint do perfil (menos `exceto`).
func sobrepoe(ctx context.Context, tx *transacao, perfil, exceto int64, inicio, fim string) error {
	var outra Sprint
	err := tx.QueryRowContext(ctx, `SELECT inicio, fim FROM sprints WHERE perfil_id = ? AND id != ? AND inicio <= ? AND fim >= ? LIMIT 1`,
		perfil, exceto, fim, inicio).Scan(&outra.Inicio, &outra.Fim)
	if errors.Is(err, sql.ErrNoRows) {
		return nil
	}
	if err != nil {
		return err
	}
	return ErrInvalido{"o período cruza a sprint de " + dataCurta(outra.Inicio) + " a " + dataCurta(outra.Fim)}
}

// dataCurta: AAAA-MM-DD vira DD/MM.
func dataCurta(d string) string {
	if len(d) != 10 {
		return d
	}
	return d[8:10] + "/" + d[5:7]
}

// CriarSprint cria uma sprint no perfil; ela não pode cruzar outra.
func (b *Banco) CriarSprint(ctx context.Context, perfil int64, inicio, fim string) (Sprint, error) {
	if err := periodoSprintValido(inicio, fim); err != nil {
		return Sprint{}, err
	}
	s := Sprint{Perfil: perfil, Inicio: inicio, Fim: fim}
	err := b.emTransacao(ctx, func(tx *transacao) error {
		var existe int
		if err := tx.QueryRowContext(ctx, `SELECT COUNT(*) FROM perfis WHERE id = ?`, perfil).Scan(&existe); err != nil {
			return err
		}
		if existe == 0 {
			return ErrNaoEncontrado
		}
		if err := sobrepoe(ctx, tx, perfil, 0, inicio, fim); err != nil {
			return err
		}
		r, err := tx.ExecContext(ctx, `INSERT INTO sprints (perfil_id, inicio, fim, criada_em) VALUES (?, ?, ?, ?)`, perfil, inicio, fim, agora())
		if err != nil {
			return err
		}
		s.ID, err = r.LastInsertId()
		return err
	})
	return s, err
}

// SprintAtual devolve a sprint do perfil que tem o dia `hoje` (AAAA-MM-DD),
// criando-a se não houver: ela segue o ritmo da última (começa no dia
// seguinte ao fim dela, com a mesma duração) ou, na primeira, é a semana que
// termina na próxima quinta, o dia da reunião. Dias sem uso entre a última e
// hoje ficam fora de qualquer sprint (dá para olhar pelos 7 ou 14 dias).
func (b *Banco) SprintAtual(ctx context.Context, perfil int64, hoje string) (Sprint, error) {
	dia, err := time.Parse("2006-01-02", hoje)
	if err != nil {
		return Sprint{}, ErrInvalido{"o dia precisa estar no formato AAAA-MM-DD"}
	}
	s, err := lerSprint(b.db.QueryRowContext(ctx, `SELECT id, perfil_id, inicio, fim FROM sprints WHERE perfil_id = ? AND inicio <= ? AND fim >= ?`, perfil, hoje, hoje))
	if !errors.Is(err, ErrNaoEncontrado) {
		return s, err
	}
	inicio, fim := proximaSprint(dia, nil)
	ultima, err := lerSprint(b.db.QueryRowContext(ctx, `SELECT id, perfil_id, inicio, fim FROM sprints WHERE perfil_id = ? ORDER BY fim DESC LIMIT 1`, perfil))
	switch {
	case err == nil:
		if ultima.Fim > hoje {
			// Hoje cai num buraco antes de uma sprint futura: a nova vai até a véspera dela.
			proxima, err := lerSprint(b.db.QueryRowContext(ctx, `SELECT id, perfil_id, inicio, fim FROM sprints WHERE perfil_id = ? AND inicio > ? ORDER BY inicio LIMIT 1`, perfil, hoje))
			if err != nil {
				return Sprint{}, err
			}
			antes, _ := time.Parse("2006-01-02", proxima.Inicio)
			inicio, fim = hoje, antes.AddDate(0, 0, -1).Format("2006-01-02")
			if anterior, err := lerSprint(b.db.QueryRowContext(ctx, `SELECT id, perfil_id, inicio, fim FROM sprints WHERE perfil_id = ? AND fim < ? ORDER BY fim DESC LIMIT 1`, perfil, hoje)); err == nil {
				depois, _ := time.Parse("2006-01-02", anterior.Fim)
				inicio = depois.AddDate(0, 0, 1).Format("2006-01-02")
			}
		} else {
			inicio, fim = proximaSprint(dia, &ultima)
		}
	case !errors.Is(err, ErrNaoEncontrado):
		return Sprint{}, err
	}
	return b.CriarSprint(ctx, perfil, inicio, fim)
}

// proximaSprint calcula o período da sprint que tem `hoje`, seguindo a
// `ultima` (que terminou antes de hoje) ou, sem ela, de sexta a quinta.
func proximaSprint(hoje time.Time, ultima *Sprint) (string, string) {
	formato := "2006-01-02"
	if ultima == nil {
		ate := hoje.AddDate(0, 0, (int(time.Thursday)-int(hoje.Weekday())+7)%7)
		return ate.AddDate(0, 0, -6).Format(formato), ate.Format(formato)
	}
	duracao := ultima.Dias()
	fimAnterior, _ := time.Parse(formato, ultima.Fim)
	// Pula os ciclos inteiros sem uso: a sprint nova é a do ciclo de hoje,
	// e começa no dia seguinte ao fim da última se for o ciclo seguinte.
	ciclos := int(hoje.Sub(fimAnterior).Hours()/24-1) / duracao
	inicio := fimAnterior.AddDate(0, 0, 1+ciclos*duracao)
	return inicio.Format(formato), inicio.AddDate(0, 0, duracao-1).Format(formato)
}

// EditarSprint muda as datas. As notas da sprint vão junto para o período
// novo (são guardadas pelo período); os títulos ficam, porque são da sprint.
func (b *Banco) EditarSprint(ctx context.Context, id int64, inicio, fim string) (Sprint, error) {
	if err := periodoSprintValido(inicio, fim); err != nil {
		return Sprint{}, err
	}
	var s Sprint
	err := b.emTransacao(ctx, func(tx *transacao) error {
		antes, err := lerSprint(tx.QueryRowContext(ctx, `SELECT id, perfil_id, inicio, fim FROM sprints WHERE id = ?`, id))
		if err != nil {
			return err
		}
		if err := sobrepoe(ctx, tx, antes.Perfil, id, inicio, fim); err != nil {
			return err
		}
		if _, err := tx.ExecContext(ctx, `UPDATE sprints SET inicio = ?, fim = ? WHERE id = ?`, inicio, fim, id); err != nil {
			return err
		}
		s = Sprint{ID: id, Perfil: antes.Perfil, Inicio: inicio, Fim: fim}
		if s.ChaveNota() == antes.ChaveNota() {
			return nil
		}
		// Uma nota que já existia no período novo (de uma consulta solta, como
		// "14 dias") dá lugar à da sprint, que é a que você escreveu para ela.
		tarefas := `SELECT t.id FROM tarefas t JOIN projetos p ON p.id = t.projeto_id JOIN workspaces w ON w.id = p.workspace_id WHERE w.perfil_id = ?`
		if _, err := tx.ExecContext(ctx, `DELETE FROM notas WHERE tipo = 'sprint' AND periodo = ? AND tarefa_id IN (`+tarefas+`)
			AND tarefa_id IN (SELECT tarefa_id FROM notas WHERE tipo = 'sprint' AND periodo = ?)`, s.ChaveNota(), antes.Perfil, antes.ChaveNota()); err != nil {
			return err
		}
		_, err = tx.ExecContext(ctx, `UPDATE notas SET periodo = ? WHERE tipo = 'sprint' AND periodo = ? AND tarefa_id IN (`+tarefas+`)`,
			s.ChaveNota(), antes.ChaveNota(), antes.Perfil)
		return err
	})
	return s, err
}

// RemoverSprint apaga a sprint e os títulos dela. As notas ficam guardadas
// pelo período (aparecem de novo se as mesmas datas forem escolhidas).
func (b *Banco) RemoverSprint(ctx context.Context, id int64) error {
	r, err := b.db.ExecContext(ctx, `DELETE FROM sprints WHERE id = ?`, id)
	if err != nil {
		return err
	}
	if n, _ := r.RowsAffected(); n == 0 {
		return ErrNaoEncontrado
	}
	return nil
}

// DefinirTituloSprint dá à tarefa um título só desta sprint (para a reunião);
// vazio volta ao título da tarefa. A tarefa precisa ser do perfil da sprint.
func (b *Banco) DefinirTituloSprint(ctx context.Context, sprint, tarefa int64, titulo string) error {
	titulo = strings.TrimSpace(titulo)
	if titulo != "" {
		var err error
		if titulo, err = nomeValido("o título", titulo, MaxTituloSprint); err != nil {
			return err
		}
	}
	return b.emTransacao(ctx, func(tx *transacao) error {
		s, err := lerSprint(tx.QueryRowContext(ctx, `SELECT id, perfil_id, inicio, fim FROM sprints WHERE id = ?`, sprint))
		if err != nil {
			return err
		}
		var projeto int64
		err = tx.QueryRowContext(ctx, `SELECT projeto_id FROM tarefas WHERE id = ?`, tarefa).Scan(&projeto)
		if errors.Is(err, sql.ErrNoRows) {
			return ErrNaoEncontrado
		}
		if err != nil {
			return err
		}
		perfil, _, err := donoDoProjeto(ctx, tx, projeto)
		if err != nil {
			return err
		}
		if perfil != s.Perfil {
			return ErrNaoEncontrado
		}
		if titulo == "" {
			_, err = tx.ExecContext(ctx, `DELETE FROM titulos_sprint WHERE sprint_id = ? AND tarefa_id = ?`, sprint, tarefa)
			return err
		}
		_, err = tx.ExecContext(ctx, `INSERT INTO titulos_sprint (sprint_id, tarefa_id, titulo) VALUES (?, ?, ?)
			ON CONFLICT (sprint_id, tarefa_id) DO UPDATE SET titulo = excluded.titulo`, sprint, tarefa, titulo)
		return err
	})
}

// TitulosDaSprint traz os títulos dados às tarefas na sprint.
func (b *Banco) TitulosDaSprint(ctx context.Context, sprint int64) (map[int64]string, error) {
	titulos := map[int64]string{}
	linhas, err := b.db.QueryContext(ctx, `SELECT tarefa_id, titulo FROM titulos_sprint WHERE sprint_id = ?`, sprint)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	for linhas.Next() {
		var id int64
		var t string
		if err := linhas.Scan(&id, &t); err != nil {
			return nil, err
		}
		titulos[id] = t
	}
	return titulos, linhas.Err()
}
