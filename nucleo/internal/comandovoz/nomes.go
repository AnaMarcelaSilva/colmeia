package comandovoz

import (
	"sort"
	"strings"
)

// Abaixo disto, o nome dito não é este candidato.
const minimo = 0.62

// Com esta semelhança e folga para o segundo, a Colmeia não pergunta "é este?".
const (
	certeza = 0.86
	folga   = 0.08
)

// Palavras que não ajudam a distinguir um nome de outro.
var vazias = map[string]bool{"a": true, "o": true, "as": true, "os": true, "de": true, "da": true, "do": true, "das": true, "dos": true,
	"e": true, "em": true, "no": true, "na": true, "com": true, "para": true, "pra": true, "um": true, "uma": true, "tarefa": true, "projeto": true}

// Candidato é um nome que pode ter sido dito: o projeto, a tarefa ou a
// configuração, com o número dela para a API.
type Candidato struct {
	ID   int64
	Nome string
	// Outro nome que vale o mesmo (o projeto e a configuração juntos, por exemplo).
	Apelidos []string
}

// Achado é um candidato com a semelhança com o que foi dito.
type Achado struct {
	Candidato
	Nota float64
}

// Busca diz o que foi encontrado para um nome dito.
type Busca struct {
	// Os candidatos acima do mínimo, do mais parecido ao menos.
	Achados []Achado
}

// Melhor é o mais parecido, se houver.
func (b Busca) Melhor() (Achado, bool) {
	if len(b.Achados) == 0 {
		return Achado{}, false
	}
	return b.Achados[0], true
}

// Certo diz se o mais parecido é claro o bastante para agir sem perguntar.
func (b Busca) Certo() bool {
	if len(b.Achados) == 0 || b.Achados[0].Nota < certeza {
		return false
	}
	return len(b.Achados) == 1 || b.Achados[0].Nota-b.Achados[1].Nota >= folga
}

// Empate são os parecidos demais com o primeiro para escolher sozinho.
func (b Busca) Empate() []Achado {
	if len(b.Achados) < 2 {
		return nil
	}
	var e []Achado
	for _, a := range b.Achados {
		if b.Achados[0].Nota-a.Nota < 0.04 {
			e = append(e, a)
		}
	}
	if len(e) < 2 {
		return nil
	}
	return e
}

// Procurar compara o nome dito com cada candidato.
func Procurar(dito string, candidatos []Candidato) Busca {
	d := normalizar(dito)
	var achados []Achado
	for _, c := range candidatos {
		nota := semelhanca(d, normalizar(c.Nome))
		for _, a := range c.Apelidos {
			nota = max(nota, semelhanca(d, normalizar(a)))
		}
		if nota >= minimo {
			achados = append(achados, Achado{Candidato: c, Nota: nota})
		}
	}
	sort.SliceStable(achados, func(i, j int) bool { return achados[i].Nota > achados[j].Nota })
	return Busca{Achados: achados}
}

// semelhanca vai de 0 a 1 entre o que foi dito e um nome, os dois já
// normalizados. Vale a melhor de três leituras: o texto inteiro (erros de
// transcrição), sem espaços ("loja web" e "lojaweb") e palavra por palavra
// (dizer só parte de um título longo).
func semelhanca(dito, nome string) float64 {
	if dito == "" || nome == "" {
		return 0
	}
	inteiro := proporcao(dito, nome)
	junto := proporcao(strings.ReplaceAll(dito, " ", ""), strings.ReplaceAll(nome, " ", ""))
	return max(inteiro, junto, porPalavras(dito, nome))
}

// porPalavras: quanto do que foi dito está no nome (cada palavra com a mais
// parecida dele), com um peso menor para quanto do nome foi dito.
func porPalavras(dito, nome string) float64 {
	pd, pn := significativas(dito), significativas(nome)
	if len(pd) == 0 || len(pn) == 0 {
		return 0
	}
	usadas := make([]bool, len(pn))
	soma := 0.0
	for _, d := range pd {
		melhor, qual := 0.0, -1
		for i, n := range pn {
			if p := proporcao(d, n); p > melhor {
				melhor, qual = p, i
			}
		}
		if melhor >= 0.75 {
			soma += melhor
			usadas[qual] = true
		}
	}
	cobertas := 0
	for _, u := range usadas {
		if u {
			cobertas++
		}
	}
	doDito := soma / float64(len(pd))
	cobertura := float64(cobertas) / float64(len(pn))
	return doDito * (0.7 + 0.3*cobertura)
}

func significativas(s string) []string {
	var r []string
	for _, p := range strings.Fields(s) {
		if !vazias[p] {
			r = append(r, p)
		}
	}
	if len(r) == 0 {
		return strings.Fields(s)
	}
	return r
}

// proporcao é 1 menos a distância de edição sobre o tamanho do maior.
func proporcao(a, b string) float64 {
	ra, rb := []rune(a), []rune(b)
	maior := max(len(ra), len(rb))
	if maior == 0 {
		return 1
	}
	return 1 - float64(distancia(ra, rb))/float64(maior)
}

// distancia de Levenshtein, com duas linhas.
func distancia(a, b []rune) int {
	antes := make([]int, len(b)+1)
	agora := make([]int, len(b)+1)
	for j := range antes {
		antes[j] = j
	}
	for i := 1; i <= len(a); i++ {
		agora[0] = i
		for j := 1; j <= len(b); j++ {
			custo := 1
			if a[i-1] == b[j-1] {
				custo = 0
			}
			agora[j] = min(antes[j]+1, agora[j-1]+1, antes[j-1]+custo)
		}
		antes, agora = agora, antes
	}
	return antes[len(b)]
}
