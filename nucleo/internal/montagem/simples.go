package montagem

import (
	"regexp"
	"strings"
	"unicode"
	"unicode/utf8"
)

// A montagem simples: sem IA, do que a tarefa já tem. A nota da daily, no
// formato da reunião, já vem em frase e itens; cada item vira um cartão, os
// passos numerados viram um cartão só (o fluxo fica junto), o "Falta" vai
// por último e as imagens mais novas entram com a legenda. Sem nota, a
// coluna leva o que aconteceu, para nunca ficar vazia.

var (
	itemDeLista  = regexp.MustCompile(`^\s*(?:[-*•]|\d+[.)])\s+`)
	itemNumerado = regexp.MustCompile(`^\s*\d+[.)]\s+`)
)

// Simples monta do que há na entrada.
func Simples(e Entrada) Montagem {
	m := Montagem{TarefaID: e.TarefaID, Origem: OrigemSimples, Base: Base(e), Topico: topicoDoTitulo(e.Titulo)}
	frase, itens := partesDaNota(e.Nota)
	if frase == "" {
		frase, itens = partesDaNota(e.Descricao)
	}
	if frase != "" {
		m.Frase = cortar(frase, MaxFrase)
		m.Cartoes = append(m.Cartoes, Cartao{Titulo: "O que mudou", Texto: cortar(frase, MaxTexto)})
	}
	var passos []string
	var falta []Cartao
	for _, it := range itens {
		switch {
		case itemNumerado.MatchString(it):
			passos = append(passos, strings.TrimSpace(it))
		case strings.HasPrefix(strings.ToLower(semMarcador(it)), "falta"):
			falta = append(falta, Cartao{Titulo: "Falta", Texto: cortar(semMarcador(it), MaxTexto)})
		default:
			texto := semMarcador(it)
			titulo := tituloDoItem(texto)
			// Um item curto é o título inteiro: o cartão fica só com o texto.
			if strings.TrimRight(titulo, ".") == strings.TrimRight(texto, ".") {
				titulo = ""
			}
			m.Cartoes = append(m.Cartoes, Cartao{Titulo: titulo, Texto: cortar(texto, MaxTexto)})
		}
	}
	if len(passos) > 0 {
		m.Cartoes = append(m.Cartoes, Cartao{Titulo: "Como funciona", Texto: cortar(strings.Join(passos, "\n"), MaxTexto)})
	}
	if m.Frase == "" {
		m.Frase = fraseDoEstado(e)
		texto := strings.Join(Resumir(e.Feito), "\n")
		if texto == "" {
			texto = m.Frase
		}
		m.Cartoes = append(m.Cartoes, Cartao{Titulo: "Onde está", Texto: cortar(texto, MaxTexto)})
	}
	// As imagens mais novas, até o limite, depois do que foi feito.
	for i, img := range e.Imagens {
		if i == MaxImagens {
			break
		}
		legenda := img.Legenda
		if legenda == "" {
			legenda = img.Nome
		}
		m.Cartoes = append(m.Cartoes, Cartao{Titulo: "Como ficou", Imagem: img.ID, Legenda: cortar(legenda, MaxLegenda)})
	}
	m.Cartoes = append(m.Cartoes, falta...)
	if len(m.Cartoes) > MaxCartoes {
		// Ficam o resumo, os primeiros itens e o fim (imagens e Falta).
		fim := len(falta)
		for _, c := range m.Cartoes {
			if c.Imagem != 0 {
				fim++
			}
		}
		cabem := MaxCartoes - fim
		if cabem < 1 {
			cabem = 1
		}
		m.Cartoes = append(m.Cartoes[:cabem:cabem], m.Cartoes[len(m.Cartoes)-fim:]...)
	}
	return m
}

// partesDaNota separa a frase (a primeira linha que não é item) dos itens.
func partesDaNota(nota string) (string, []string) {
	frase := ""
	var itens []string
	for _, linha := range strings.Split(strings.ReplaceAll(nota, "\r\n", "\n"), "\n") {
		linha = strings.TrimRight(linha, " \t")
		if strings.TrimSpace(linha) == "" {
			continue
		}
		if itemDeLista.MatchString(linha) {
			itens = append(itens, linha)
			continue
		}
		if frase == "" {
			frase = strings.TrimSpace(semMarkdown(linha))
			continue
		}
		// Um parágrafo a mais é tratado como item.
		itens = append(itens, "- "+strings.TrimSpace(linha))
	}
	return frase, itens
}

func semMarcador(item string) string {
	return strings.TrimSpace(semMarkdown(itemDeLista.ReplaceAllString(item, "")))
}

// semMarkdown tira títulos e negrito de uma linha.
func semMarkdown(linha string) string {
	linha = strings.TrimLeft(linha, "#")
	return strings.ReplaceAll(linha, "**", "")
}

// tituloDoItem: o item até a primeira vírgula, dois-pontos ou ponto, em
// até MaxTitulo ("Correção da transferência de comanda").
func tituloDoItem(texto string) string {
	corte := len(texto)
	for i, r := range texto {
		if r == ',' || r == ':' || r == ';' || (r == '.' && i+1 < len(texto)) {
			corte = i
			break
		}
	}
	titulo := strings.TrimSpace(texto[:corte])
	titulo = strings.TrimRight(titulo, ".")
	if utf8.RuneCountInString(titulo) > MaxTitulo {
		titulo = primeirasPalavras(titulo, 5)
	}
	return cortar(titulo, MaxTitulo)
}

// topicoDoTitulo: o título da tarefa inteiro quando é curto; senão as
// primeiras palavras, sem terminar numa palavra de ligação ("Paginação na
// lista de" vira "Paginação na lista").
func topicoDoTitulo(titulo string) string {
	palavras := strings.FieldsFunc(strings.TrimSpace(titulo), unicode.IsSpace)
	if len(palavras) == 0 {
		return "Tarefa"
	}
	if len(palavras) > 5 {
		palavras = palavras[:5]
	}
	for len(palavras) > 1 && ligacao[strings.ToLower(strings.Trim(palavras[len(palavras)-1], ",.;:"))] {
		palavras = palavras[:len(palavras)-1]
	}
	return cortar(strings.Join(palavras, " "), MaxTopico)
}

// As palavras em que um tópico não termina.
var ligacao = map[string]bool{"de": true, "da": true, "do": true, "das": true, "dos": true, "na": true, "no": true, "nas": true, "nos": true,
	"em": true, "com": true, "por": true, "para": true, "pra": true, "e": true, "ou": true, "a": true, "o": true, "as": true, "os": true,
	"um": true, "uma": true, "que": true, "enquanto": true, "ao": true, "à": true, "sem": true, "sobre": true}

func primeirasPalavras(texto string, n int) string {
	palavras := strings.FieldsFunc(texto, unicode.IsSpace)
	if len(palavras) <= n {
		return strings.Join(palavras, " ")
	}
	return strings.Join(palavras[:n], " ")
}

func fraseDoEstado(e Entrada) string {
	switch e.Estado {
	case "Concluída":
		return "Concluída."
	case "Em revisão":
		return "Pronta para revisão."
	case "Esperando você":
		return "Em andamento, esperando uma resposta."
	case "Parou com erro":
		return "Parou com erro; falta retomar."
	}
	return "Em andamento."
}

func cortar(texto string, maximo int) string {
	texto = strings.TrimSpace(texto)
	if utf8.RuneCountInString(texto) <= maximo {
		return texto
	}
	return strings.TrimSpace(string([]rune(texto)[:maximo-1])) + "…"
}
