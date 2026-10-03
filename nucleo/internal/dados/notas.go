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

// Modos de gravar uma nota: substituir o texto ou acrescentar ao fim dele.
const (
	ModoSubstituir   = "substituir"
	ModoComplementar = "complementar"
)

// GravacaoNota é um pedido de gravar uma nota. Versao (opcional) é o
// atualizada_em que quem edita leu ("" se a nota não existia): se a nota
// mudou desde então, nada é gravado e volta ErrNotaMudou. Agente diz que foi
// um agente quem escreveu (pelas ferramentas da Colmeia).
type GravacaoNota struct {
	Tarefa        int64
	Tipo, Periodo string
	Texto         string
	Modo          string
	Versao        *string
	Agente        int64
}

// ErrNotaMudou: a nota mudou desde que foi lida. Atual é como ela está agora.
type ErrNotaMudou struct{ Atual Nota }

func (e ErrNotaMudou) Error() string { return "a nota mudou enquanto você editava" }

// DefinirNota grava (ou, com texto vazio, apaga) a nota da tarefa. O evento
// leva só o tamanho do texto: o conteúdo não entra no histórico imutável.
func (b *Banco) DefinirNota(ctx context.Context, tarefa int64, tipo, periodo, texto string) (Nota, error) {
	return b.GravarNota(ctx, GravacaoNota{Tarefa: tarefa, Tipo: tipo, Periodo: periodo, Texto: texto})
}

func textoDeNota(texto string) (string, error) {
	texto = strings.TrimRight(texto, " \t\n")
	if !utf8.ValidString(texto) || strings.ContainsFunc(texto, func(r rune) bool { return (r < ' ' && r != '\n' && r != '\t') || r == 0x7f }) {
		return "", ErrInvalido{"a nota tem caracteres de controle"}
	}
	// A nota aparece no telão e vai para o texto da daily: nada que pareça
	// senha ou chave é gravado (o mesmo filtro do histórico de mensagens).
	if segredos.Parece(texto) {
		return "", ErrInvalido{"a nota parece ter uma senha ou chave"}
	}
	return texto, nil
}

// GravarNota substitui ou complementa a nota. Complementar acrescenta o texto
// ao fim, depois de uma linha em branco, e o resultado também respeita o
// limite de MaxNota.
func (b *Banco) GravarNota(ctx context.Context, g GravacaoNota) (Nota, error) {
	if err := PeriodoNotaValido(g.Tipo, g.Periodo); err != nil {
		return Nota{}, err
	}
	modo := g.Modo
	if modo == "" {
		modo = ModoSubstituir
	}
	if err := umDe("modo", modo, []string{ModoSubstituir, ModoComplementar}); err != nil {
		return Nota{}, err
	}
	texto, err := textoDeNota(g.Texto)
	if err != nil {
		return Nota{}, err
	}
	if modo == ModoComplementar && strings.TrimSpace(texto) == "" {
		return Nota{}, ErrInvalido{"o texto para complementar a nota está vazio"}
	}
	if utf8.RuneCountInString(texto) > MaxNota {
		return Nota{}, ErrInvalido{"a nota pode ter no máximo 4.000 caracteres"}
	}
	n := Nota{TarefaID: g.Tarefa, Tipo: g.Tipo, Periodo: g.Periodo, AtualizadaEm: agora()}
	err = b.emTransacao(ctx, func(tx *transacao) error {
		var projeto int64
		var titulo string
		err := tx.QueryRowContext(ctx, `SELECT projeto_id, titulo FROM tarefas WHERE id = ?`, g.Tarefa).Scan(&projeto, &titulo)
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
		var antes, versao string
		err = tx.QueryRowContext(ctx, `SELECT texto, atualizada_em FROM notas WHERE tarefa_id = ? AND tipo = ? AND periodo = ?`, g.Tarefa, g.Tipo, g.Periodo).Scan(&antes, &versao)
		if err != nil && !errors.Is(err, sql.ErrNoRows) {
			return err
		}
		if g.Versao != nil && *g.Versao != versao {
			return ErrNotaMudou{Atual: Nota{TarefaID: g.Tarefa, Tipo: g.Tipo, Periodo: g.Periodo, Texto: antes, AtualizadaEm: versao}}
		}
		n.Texto = texto
		if modo == ModoComplementar && antes != "" {
			n.Texto = antes + "\n\n" + texto
			if utf8.RuneCountInString(n.Texto) > MaxNota {
				return ErrInvalido{"a nota passaria de 4.000 caracteres; resuma"}
			}
		}
		if antes == n.Texto {
			n.AtualizadaEm = versao
			return nil // nada mudou: nenhum evento
		}
		if n.Texto == "" {
			_, err = tx.ExecContext(ctx, `DELETE FROM notas WHERE tarefa_id = ? AND tipo = ? AND periodo = ?`, g.Tarefa, g.Tipo, g.Periodo)
			n.AtualizadaEm = ""
		} else {
			_, err = tx.ExecContext(ctx, `INSERT INTO notas (tarefa_id, tipo, periodo, texto, atualizada_em) VALUES (?, ?, ?, ?, ?)
				ON CONFLICT (tarefa_id, tipo, periodo) DO UPDATE SET texto = excluded.texto, atualizada_em = excluded.atualizada_em`,
				g.Tarefa, g.Tipo, g.Periodo, n.Texto, n.AtualizadaEm)
		}
		if err != nil {
			return err
		}
		conteudo := map[string]any{
			"tarefa": g.Tarefa, "titulo": titulo, "projeto_nome": nomeProjeto, "tipo": g.Tipo, "periodo": g.Periodo, "tamanho": utf8.RuneCountInString(n.Texto),
		}
		escopo := Escopo{Perfil: perfil, Projeto: projeto, Tarefa: g.Tarefa}
		if g.Agente != 0 {
			escopo.Agente = g.Agente
			conteudo["agente"], conteudo["modo"] = g.Agente, modo
		}
		return registrar(ctx, tx, "nota.atualizada", escopo, conteudo)
	})
	return n, err
}

// Nota lê uma nota; uma que não existe volta vazia, sem erro.
func (b *Banco) Nota(ctx context.Context, tarefa int64, tipo, periodo string) (Nota, error) {
	if err := PeriodoNotaValido(tipo, periodo); err != nil {
		return Nota{}, err
	}
	n := Nota{TarefaID: tarefa, Tipo: tipo, Periodo: periodo}
	err := b.db.QueryRowContext(ctx, `SELECT texto, atualizada_em FROM notas WHERE tarefa_id = ? AND tipo = ? AND periodo = ?`, tarefa, tipo, periodo).Scan(&n.Texto, &n.AtualizadaEm)
	if errors.Is(err, sql.ErrNoRows) {
		return n, nil
	}
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
