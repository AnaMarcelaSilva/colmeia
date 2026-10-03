package dados

import (
	"context"
	"database/sql"
	"errors"
	"regexp"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/segredos"
)

// MaxNota em caracteres.
const MaxNota = 4000

var (
	TiposNota    = []string{"daily", "sprint"}
	padraoDia    = regexp.MustCompile(`^\d{4}-\d{2}-\d{2}$`)
	padraoPeriod = regexp.MustCompile(`^(\d{4}-\d{2}-\d{2})\.\.(\d{4}-\d{2}-\d{2})$`)
)

// Nota é o que você escreveu sobre uma tarefa para uma daily (periodo é o
// dia, AAAA-MM-DD) ou uma sprint (AAAA-MM-DD..AAAA-MM-DD).
type Nota struct {
	TarefaID     int64  `json:"tarefa_id"`
	Tipo         string `json:"tipo"`
	Periodo      string `json:"periodo"`
	Texto        string `json:"texto"`
	AtualizadaEm string `json:"atualizada_em"`
}

func dataReal(v string) bool {
	_, err := time.Parse("2006-01-02", v)
	return err == nil
}

// PeriodoNotaValido confere o período de uma nota pelo tipo.
func PeriodoNotaValido(tipo, periodo string) error {
	if err := umDe("tipo", tipo, TiposNota); err != nil {
		return err
	}
	if tipo == "daily" {
		if !padraoDia.MatchString(periodo) || !dataReal(periodo) {
			return ErrInvalido{"o período da daily precisa ser uma data AAAA-MM-DD"}
		}
		return nil
	}
	partes := padraoPeriod.FindStringSubmatch(periodo)
	if partes == nil || !dataReal(partes[1]) || !dataReal(partes[2]) || partes[2] < partes[1] {
		return ErrInvalido{"o período da sprint precisa ser AAAA-MM-DD..AAAA-MM-DD"}
	}
	return nil
}

// DefinirNota grava (ou, com texto vazio, apaga) a nota da tarefa. O evento
// leva só o tamanho do texto: o conteúdo não entra no histórico imutável.
func (b *Banco) DefinirNota(ctx context.Context, tarefa int64, tipo, periodo, texto string) (Nota, error) {
	if err := PeriodoNotaValido(tipo, periodo); err != nil {
		return Nota{}, err
	}
	texto = strings.TrimRight(texto, " \t\n")
	if utf8.RuneCountInString(texto) > MaxNota {
		return Nota{}, ErrInvalido{"a nota pode ter no máximo 4.000 caracteres"}
	}
	if !utf8.ValidString(texto) || strings.ContainsFunc(texto, func(r rune) bool { return (r < ' ' && r != '\n' && r != '\t') || r == 0x7f }) {
		return Nota{}, ErrInvalido{"a nota tem caracteres de controle"}
	}
	// A nota aparece no telão e vai para o texto da daily: nada que pareça
	// senha ou chave é gravado (o mesmo filtro do histórico de mensagens).
	if segredos.Parece(texto) {
		return Nota{}, ErrInvalido{"a nota parece ter uma senha ou chave"}
	}
	n := Nota{TarefaID: tarefa, Tipo: tipo, Periodo: periodo, Texto: texto, AtualizadaEm: agora()}
	err := b.emTransacao(ctx, func(tx *transacao) error {
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
		var antes string
		err = tx.QueryRowContext(ctx, `SELECT texto FROM notas WHERE tarefa_id = ? AND tipo = ? AND periodo = ?`, tarefa, tipo, periodo).Scan(&antes)
		if err != nil && !errors.Is(err, sql.ErrNoRows) {
			return err
		}
		if antes == texto {
			return nil // nada mudou: nenhum evento
		}
		if texto == "" {
			_, err = tx.ExecContext(ctx, `DELETE FROM notas WHERE tarefa_id = ? AND tipo = ? AND periodo = ?`, tarefa, tipo, periodo)
		} else {
			_, err = tx.ExecContext(ctx, `INSERT INTO notas (tarefa_id, tipo, periodo, texto, atualizada_em) VALUES (?, ?, ?, ?, ?)
				ON CONFLICT (tarefa_id, tipo, periodo) DO UPDATE SET texto = excluded.texto, atualizada_em = excluded.atualizada_em`,
				tarefa, tipo, periodo, texto, n.AtualizadaEm)
		}
		if err != nil {
			return err
		}
		return registrar(ctx, tx, "nota.atualizada", Escopo{Perfil: perfil, Projeto: projeto, Tarefa: tarefa}, map[string]any{
			"tarefa": tarefa, "titulo": titulo, "projeto_nome": nomeProjeto, "tipo": tipo, "periodo": periodo, "tamanho": utf8.RuneCountInString(texto),
		})
	})
	return n, err
}

// NotasDoPeriodo traz as notas das tarefas para o tipo e período, pelo id da tarefa.
func (b *Banco) NotasDoPeriodo(ctx context.Context, tarefas []int64, tipo, periodo string) (map[int64]Nota, error) {
	notas := map[int64]Nota{}
	if len(tarefas) == 0 {
		return notas, nil
	}
	args := []any{tipo, periodo}
	for _, t := range tarefas {
		args = append(args, t)
	}
	linhas, err := b.db.QueryContext(ctx, `SELECT tarefa_id, tipo, periodo, texto, atualizada_em FROM notas
		WHERE tipo = ? AND periodo = ? AND tarefa_id IN (?`+strings.Repeat(`, ?`, len(tarefas)-1)+`)`, args...)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	for linhas.Next() {
		var n Nota
		if err := linhas.Scan(&n.TarefaID, &n.Tipo, &n.Periodo, &n.Texto, &n.AtualizadaEm); err != nil {
			return nil, err
		}
		notas[n.TarefaID] = n
	}
	return notas, linhas.Err()
}

// UltimasNotas traz, por tarefa, a nota mais recente do tipo, de qualquer
// período: a sprint mostra a anterior quando o período mudou.
func (b *Banco) UltimasNotas(ctx context.Context, tarefas []int64, tipo string) (map[int64]Nota, error) {
	notas := map[int64]Nota{}
	if len(tarefas) == 0 {
		return notas, nil
	}
	args := []any{tipo}
	for _, t := range tarefas {
		args = append(args, t)
	}
	linhas, err := b.db.QueryContext(ctx, `SELECT tarefa_id, tipo, periodo, texto, atualizada_em FROM notas
		WHERE tipo = ? AND tarefa_id IN (?`+strings.Repeat(`, ?`, len(tarefas)-1)+`) ORDER BY atualizada_em`, args...)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	for linhas.Next() {
		var n Nota
		if err := linhas.Scan(&n.TarefaID, &n.Tipo, &n.Periodo, &n.Texto, &n.AtualizadaEm); err != nil {
			return nil, err
		}
		notas[n.TarefaID] = n // a ordem deixa a mais recente por último
	}
	return notas, linhas.Err()
}
