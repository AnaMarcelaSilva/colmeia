package montagem

import (
	"fmt"
	"math"
	"strings"
)

// O desenho da lousa da reunião: cada tarefa é uma coluna. No alto o cartão
// da tarefa; abaixo, os cartões ligados por setas para baixo; o último liga
// ao cartão da tarefa da coluna seguinte, para o palco passar de uma tarefa
// à outra sozinho. Acima de cada grupo de colunas, o nome da seção (o
// projeto, ou o assunto na sprint), um texto solto que não entra no palco.

const (
	LarguraColuna = 480.0
	EntreColunas  = 56.0
	EntreCartoes  = 48.0
	MargemEsq     = 80.0
	MargemTopo    = 80.0
	AlturaSecao   = 40.0
	AlturaTarefa  = 112.0
	alturaMinima  = 96.0
	alturaMaxima  = 560.0
	// Letras por linha e altura da linha num cartão de 480 (estimativa da tela).
	letrasPorLinha = 52.0
	alturaLinha    = 22.0
	alturaTitulo   = 44.0
	alturaLegenda  = 36.0
)

// Coluna é uma tarefa na lousa da reunião, na ordem da reunião.
type Coluna struct {
	Montagem
	// Secao é o nome do grupo (projeto ou assunto); SecaoID o separa de
	// outro com o mesmo nome.
	Secao   string
	SecaoID int64
	// Imagens: o tamanho de cada anexo, para a altura do cartão.
	Imagens map[int64]Imagem
}

// Peca é um item a criar na lousa, já posicionado. Os refs ligam as peças
// entre si dentro do mesmo lote.
type Peca struct {
	Ref       string
	Tipo      string
	X, Y      float64
	Largura   float64
	Altura    float64
	Cor       string
	Titulo    string
	Texto     string
	AnexoID   int64
	TarefaRef int64
	De, Para  string
	// TarefaID é de quem é a peça (0 no rótulo da seção).
	TarefaID int64
}

// Desenhar posiciona as colunas e devolve as peças, na ordem de criação (as
// ligações depois das pontas delas).
func Desenhar(colunas []Coluna) []Peca {
	var pecas []Peca
	x := MargemEsq
	var secaoAtual int64
	secaoAberta := false
	inicioSecao := x
	fecharSecao := func(nome string, fim float64) {
		if !secaoAberta {
			return
		}
		pecas = append(pecas, Peca{Ref: fmt.Sprintf("s%d", secaoAtual), Tipo: "texto", X: inicioSecao, Y: MargemTopo,
			Largura: math.Max(fim-inicioSecao, LarguraColuna), Altura: AlturaSecao, Cor: "cinza", Texto: nome})
	}
	ultimoRef := ""
	secaoNome := ""
	for i, c := range colunas {
		if i == 0 || c.SecaoID != secaoAtual || c.Secao != secaoNome {
			fecharSecao(secaoNome, x-EntreColunas)
			secaoAtual, secaoNome, secaoAberta, inicioSecao = c.SecaoID, c.Secao, c.Secao != "", x
		}
		y := MargemTopo + AlturaSecao + EntreCartoes/2
		refTarefa := fmt.Sprintf("t%d", c.TarefaID)
		pecas = append(pecas, Peca{Ref: refTarefa, Tipo: "tarefa", X: x, Y: y, Largura: LarguraColuna, Altura: AlturaTarefa, Cor: "cinza", TarefaRef: c.TarefaID, TarefaID: c.TarefaID})
		if ultimoRef != "" {
			pecas = append(pecas, Peca{Ref: refTarefa + "-de-" + ultimoRef, Tipo: "ligacao", De: ultimoRef, Para: refTarefa, TarefaID: c.TarefaID})
		}
		anterior := refTarefa
		y += AlturaTarefa + EntreCartoes
		for j, cartao := range c.Cartoes {
			ref := fmt.Sprintf("t%dc%d", c.TarefaID, j)
			p := Peca{Ref: ref, Tipo: "nota", X: x, Y: y, Largura: LarguraColuna, Cor: corDoCartao(j, cartao), Titulo: cartao.Titulo, Texto: cartao.Texto, TarefaID: c.TarefaID}
			if cartao.Imagem != 0 {
				p.Tipo, p.AnexoID, p.Titulo, p.Texto = "imagem", cartao.Imagem, cartao.Legenda, ""
				if p.Titulo == "" {
					p.Titulo = cartao.Titulo
				}
				p.Altura = alturaDaImagem(c.Imagens[cartao.Imagem])
			} else {
				p.Altura = alturaDoTexto(cartao.Texto)
			}
			pecas = append(pecas, p, Peca{Ref: ref + "-de-" + anterior, Tipo: "ligacao", De: anterior, Para: ref, TarefaID: c.TarefaID})
			anterior = ref
			y += p.Altura + EntreCartoes
		}
		ultimoRef = anterior
		x += LarguraColuna + EntreColunas
	}
	fecharSecao(secaoNome, x-EntreColunas)
	return pecas
}

// corDoCartao: o resumo em azul, o que falta em rosa, o resto amarelo.
func corDoCartao(indice int, c Cartao) string {
	switch {
	case strings.EqualFold(c.Titulo, "Falta"):
		return "rosa"
	case indice == 0:
		return "azul"
	}
	return "amarelo"
}

// alturaDoTexto estima a altura de uma nota de 480 de largura com o título.
func alturaDoTexto(texto string) float64 {
	linhas := 0.0
	if strings.TrimSpace(texto) == "" {
		return alturaMinima
	}
	for _, l := range strings.Split(texto, "\n") {
		n := float64(len([]rune(l)))
		linhas += math.Max(1, math.Ceil(n/letrasPorLinha))
	}
	return math.Min(alturaMaxima, math.Max(alturaMinima, alturaTitulo+linhas*alturaLinha+16))
}

// alturaDaImagem mantém a proporção do anexo na largura da coluna, com a
// legenda embaixo.
func alturaDaImagem(img Imagem) float64 {
	if img.Largura <= 0 || img.Altura <= 0 {
		return LarguraColuna*9/16 + alturaLegenda
	}
	return math.Min(alturaMaxima, math.Max(alturaMinima, LarguraColuna*float64(img.Altura)/float64(img.Largura)+alturaLegenda))
}
