package comandovoz

import (
	"strings"
)

// A gramática fechada da lista: um verbo e o número do item (1 a 9, por
// extenso, ordinal ou em algarismo), ou uma palavra só (próxima, anterior,
// repetir, fechar). Em vez de regras, a frase transcrita é comparada com
// todas as frases válidas e vale a mais parecida, pela escrita e por um
// jeito simplificado de soar ("hum" e "um", "tres" e "treis"): o Whisper
// pode errar uma letra ou outra e o pedido ainda é entendido. Sem confiança
// (pouco parecida, ou duas frases de sentidos diferentes perto demais), a
// abelha pergunta de novo.

// Verbo da lista.
type Verbo int

const (
	SemVerbo Verbo = iota
	VAbrir
	VResponder
	VConcluir
	VRevisao
	VParar
	VRodar
	VProxima
	VAnterior
	VRepetir
	VFechar
	VSim
	VNao
)

// Escolha é a frase da gramática mais parecida com a transcrita.
type Escolha struct {
	Verbo  Verbo
	Numero int
	// Nota de 0 a 1 da frase escolhida, e se ela é clara o bastante para agir.
	Nota  float64
	Certa bool
	// ForaDaLista: um número dito com todas as letras, mas que a página não tem.
	ForaDaLista int
}

// Com esta semelhança e esta folga para a melhor frase de outro sentido, vale.
const (
	notaGramatica  = 0.72
	folgaGramatica = 0.08
	// Abaixo disto a frase não parece um pedido da lista (pode ser um
	// comando por nome ou uma pergunta livre).
	PareceLista = 0.55
)

// As formas de cada número: por extenso, no feminino e ordinal.
var formasNumero = [][]string{
	1: {"um", "uma", "primeiro", "primeira"},
	2: {"dois", "duas", "segundo", "segunda"},
	3: {"tres", "terceiro", "terceira"},
	4: {"quatro", "quarto", "quarta"},
	5: {"cinco", "quinto", "quinta"},
	6: {"seis", "sexto", "sexta"},
	7: {"sete", "setimo", "setima"},
	8: {"oito", "oitavo", "oitava"},
	9: {"nove", "nono", "nona"},
}

// Os verbos que pedem o número, com as formas que valem por eles.
var verbosComNumero = []struct {
	verbo  Verbo
	formas []string
}{
	{VAbrir, []string{"abrir", "abre", "abra", "mostrar", "mostra", "ver"}},
	{VResponder, []string{"responder", "responde", "responda"}},
	{VConcluir, []string{"concluir", "conclui", "conclua", "encerrar", "encerra", "finalizar"}},
	{VRevisao, []string{"enviar para revisao", "envia para revisao", "mandar para revisao", "revisao", "revisar"}},
	{VParar, []string{"parar", "para", "pare"}},
	{VRodar, []string{"rodar", "roda", "rode", "executar"}},
}

// Os pedidos de uma palavra (ou duas), sem número.
var semNumero = []struct {
	verbo  Verbo
	formas []string
}{
	{VProxima, []string{"proxima", "proxima pagina", "seguinte", "mais"}},
	{VAnterior, []string{"anterior", "pagina anterior", "voltar", "volta"}},
	{VRepetir, []string{"repetir", "repete", "repita", "de novo", "fala de novo", "outra vez"}},
	{VFechar, []string{"fechar", "fecha", "cancelar", "cancela", "sair", "nada", "esquece"}},
}

var confirmacao = []struct {
	verbo  Verbo
	formas []string
}{
	{VSim, []string{"sim", "pode", "pode sim", "sim pode", "confirmo", "confirma", "isso"}},
	{VNao, []string{"nao", "nao quero"}},
}

// Palavras que não mudam o pedido: "abrir a tarefa número um" é "abrir um".
var enchimento = map[string]bool{"a": true, "o": true, "as": true, "os": true, "ao": true, "aos": true, "na": true, "no": true,
	"da": true, "do": true, "de": true, "e": true, "numero": true, "item": true, "tarefa": true, "projeto": true, "opcao": true,
	"linha": true, "agora": true}

var algarismos = map[rune]string{'1': "um", '2': "dois", '3': "tres", '4': "quatro", '5': "cinco", '6': "seis", '7': "sete", '8': "oito", '9': "nove"}

// limpar normaliza e tira o nome da abelha, as cortesias e o enchimento;
// um algarismo sozinho ("1", "2a") vira o número por extenso.
func limpar(frase, nomeAbelha string) string {
	t := tirarNome(normalizar(frase), normalizar(nomeAbelha))
	t = fimSobrando.ReplaceAllString(inicioSobrando.ReplaceAllString(t, ""), "")
	var palavras []string
	for _, p := range strings.Fields(t) {
		if r := []rune(p); len(r) >= 1 && algarismos[r[0]] != "" && (len(r) == 1 || (len(r) == 2 && strings.ContainsRune("oaºª", r[1]))) {
			p = algarismos[r[0]]
		}
		if !enchimento[p] {
			palavras = append(palavras, p)
		}
	}
	return strings.Join(palavras, " ")
}

// frase da gramática, já limpa, com o sentido dela.
type frase struct {
	texto  string
	verbo  Verbo
	numero int
}

// frases monta as frases válidas para uma página de `quantos` itens; com
// uma pergunta no ar, "sim" e "não" também valem.
func frases(quantos int, comPergunta bool) []frase {
	var fs []frase
	add := func(texto string, v Verbo, n int) {
		fs = append(fs, frase{texto: limpar(texto, ""), verbo: v, numero: n})
	}
	for n := 1; n <= min(quantos, len(formasNumero)-1); n++ {
		for _, numero := range formasNumero[n] {
			for _, v := range verbosComNumero {
				for _, forma := range v.formas {
					add(forma+" "+numero, v.verbo, n)
				}
			}
			add("enviar "+numero+" para revisao", VRevisao, n)
			add("mandar "+numero+" para revisao", VRevisao, n)
			// O número sozinho abre.
			add(numero, VAbrir, n)
		}
	}
	for _, v := range semNumero {
		for _, forma := range v.formas {
			add(forma, v.verbo, 0)
		}
	}
	if comPergunta {
		for _, v := range confirmacao {
			for _, forma := range v.formas {
				add(forma, v.verbo, 0)
			}
		}
	}
	return fs
}

// soar simplifica a escrita para o som aproximado do português: o que o
// Whisper troca por soar igual ("hum", "treis", "cesta") fica igual.
func soar(s string) string {
	troca := strings.NewReplacer(
		"ch", "x", "lh", "li", "nh", "ni", "qu", "k", "gu", "g", "ss", "s", "rr", "r",
		"ce", "se", "ci", "si", "ca", "ka", "co", "ko", "cu", "ku", "z", "s", "h", "", "y", "i", "w", "v",
		"ei", "e", "ou", "o",
	)
	s = troca.Replace(s)
	// O "e" e o "o" no fim da palavra soam "i" e "u".
	palavras := strings.Fields(s)
	for i, p := range palavras {
		switch {
		case strings.HasSuffix(p, "e") && len(p) > 2:
			palavras[i] = p[:len(p)-1] + "i"
		case strings.HasSuffix(p, "o") && len(p) > 2:
			palavras[i] = p[:len(p)-1] + "u"
		}
	}
	return strings.Join(palavras, " ")
}

// parecida é a melhor de três leituras: a escrita, a escrita sem espaços
// ("abriram" e "abrir um") e o som.
func parecida(dito, f string) float64 {
	return max(proporcao(dito, f), proporcao(strings.ReplaceAll(dito, " ", ""), strings.ReplaceAll(f, " ", "")), proporcao(soar(dito), soar(f)))
}

// EscolherDaLista acha a frase da gramática mais parecida com a transcrita,
// para uma página com `quantos` itens.
func EscolherDaLista(transcrito, nomeAbelha string, quantos int, comPergunta bool) Escolha {
	dito := limpar(transcrito, nomeAbelha)
	if dito == "" {
		return Escolha{}
	}
	// A melhor nota de cada sentido; vale o primeiro, com folga para o segundo.
	type sentido struct {
		verbo  Verbo
		numero int
	}
	notas := map[sentido]float64{}
	for _, f := range frases(quantos, comPergunta) {
		k := sentido{f.verbo, f.numero}
		notas[k] = max(notas[k], parecida(dito, f.texto))
	}
	var melhor Escolha
	melhorOutro := 0.0
	for k, nota := range notas {
		if nota > melhor.Nota || (nota == melhor.Nota && (k.verbo < melhor.Verbo || (k.verbo == melhor.Verbo && k.numero < melhor.Numero))) {
			melhorOutro = max(melhorOutro, melhor.Nota)
			melhor = Escolha{Verbo: k.verbo, Numero: k.numero, Nota: nota}
		} else {
			melhorOutro = max(melhorOutro, nota)
		}
	}
	// A folga conta em nota ou em proporção do erro: numa frase longa, uma
	// letra a mais pesa pouco na nota ("quarta" e "quinta" no fim de
	// "enviar para revisão"), mas o outro sentido ainda erra bem mais.
	folga := melhor.Nota - melhorOutro
	melhor.Certa = melhor.Nota >= notaGramatica && folga > 0.02 && (folga >= folgaGramatica || 1-melhorOutro >= 2.5*(1-melhor.Nota))
	if !melhor.Certa {
		melhor.ForaDaLista = numeroFora(dito, quantos)
	}
	return melhor
}

// numeroFora acha um número escrito com todas as letras que passa da página.
func numeroFora(dito string, quantos int) int {
	for _, p := range strings.Fields(dito) {
		for n := quantos + 1; n < len(formasNumero); n++ {
			for _, forma := range formasNumero[n] {
				if p == forma {
					return n
				}
			}
		}
	}
	return 0
}

// PareceNumero diz se o nome dito num comando ("abrir a um") é, na
// verdade, um número da lista: aí não se procura uma tarefa com esse nome.
func PareceNumero(alvo string) bool {
	t := limpar(alvo, "")
	if t == "" {
		return false
	}
	for n := 1; n < len(formasNumero); n++ {
		for _, forma := range formasNumero[n] {
			if parecida(t, forma) >= 0.75 {
				return true
			}
		}
	}
	return false
}
