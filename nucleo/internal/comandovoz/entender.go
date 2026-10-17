// Package comandovoz entende os comandos falados à abelha: uma frase já
// transcrita (o colmeia-voz transcreve; o núcleo nunca ouve áudio) vira um
// pedido de uma lista fixa, e a resposta é montada com o que o núcleo já
// sabe do quadro. Sem IA e sem rede: regras simples em português e busca
// aproximada de nomes de projeto e tarefa. Nada aqui executa nada: quem
// executa (e pede a confirmação) é a API.
package comandovoz

import (
	"regexp"
	"strings"
	"unicode"

	"golang.org/x/text/unicode/norm"
)

// Intencao é o que a pessoa pediu.
type Intencao int

const (
	Desconhecida Intencao = iota
	Rodar
	Parar
	Encerrar
	Abrir
	ComoEsta
	OQueEspera
	Sim
	Nao
	// SoNome: só o nome da abelha, sem pedido.
	SoNome
)

// Pedido é a frase entendida: a intenção e o nome dito (projeto ou tarefa).
// Duvida: o verbo veio corrigido ("codar" virou "rodar"); a ação pergunta antes.
type Pedido struct {
	Intencao Intencao
	Alvo     string
	Duvida   bool
}

// normalizar deixa a frase só com letras e números minúsculos, sem acento
// e com um espaço entre as palavras: "Rodar a Loja-Web!" vira "rodar a loja web".
func normalizar(s string) string {
	var b strings.Builder
	for _, r := range norm.NFD.String(strings.ToLower(s)) {
		switch {
		case unicode.Is(unicode.Mn, r):
			// O acento separado da letra pelo NFD.
		case unicode.IsLetter(r) || unicode.IsDigit(r):
			b.WriteRune(r)
		default:
			b.WriteRune(' ')
		}
	}
	return strings.Join(strings.Fields(b.String()), " ")
}

// Palavras de cortesia e chamados que não mudam o pedido.
var (
	inicioSobrando = regexp.MustCompile(`^(?:(?:ei|oi|ola|ok|e ai|bom dia|boa tarde|boa noite|por favor|favor|voce pode|voce poderia|pode|poderia|consegue|quero|eu quero|queria|eu queria|gostaria de|preciso|eu preciso|agora|entao|tipo)\s+)+`)
	fimSobrando    = regexp.MustCompile(`(?:\s+(?:por favor|pra mim|para mim|agora|ai|obrigad[oa]|valeu))+$`)
)

// Os pedidos, na ordem em que são testados. O nome vem no grupo "alvo".
var regras = []struct {
	intencao Intencao
	padrao   *regexp.Regexp
}{
	{OQueEspera, regexp.MustCompile(`\b(?:espera|esperar|esperando|aguarda|aguardando)\b.*\bpor mim\b|^(?:o que|que|quem|tem algo|tem alguma coisa|alguma coisa|algo)\b.*\b(?:espera|esperando|aguarda|aguardando|precisa)\b.*\b(?:mim|me|eu)\b|^o que (?:tem|ha|falta)(?: (?:pra|para) (?:mim|eu))?$|^(?:tem|ha) (?:algo|alguma coisa) (?:pra|para) (?:mim|eu)\b`)},
	{ComoEsta, regexp.MustCompile(`^(?:como (?:esta|vai|anda|ficou|que esta)|qual (?:e )?(?:o )?(?:status|estado|situacao|andamento)|status|situacao|andamento)(?: (?:de|da|do))?(?: (?:a|o))?(?: tarefa)?(?: (?:de|da|do))? (?P<alvo>.+)$`)},
	{Abrir, regexp.MustCompile(`^(?:abr[aei]r?|abre|mostr[ae]r?|mostra|me mostr[ae]|ir (?:para|pra)|vai (?:para|pra)|va (?:para|pra)|entr[ae]r? n[ao])(?: (?:a|o))?(?: tarefa)?(?: (?:de|da|do))? (?P<alvo>.+)$`)},
	{Encerrar, regexp.MustCompile(`^(?:encerr[ae]r?|encerra|conclu(?:ir|a|i)|finaliz[ae]r?|finaliza|termin[ae]r?|termina|fech[ae]r?|fecha)(?: (?:a|o))?(?: tarefa)?(?: (?:de|da|do))? (?P<alvo>.+)$`)},
	{Rodar, regexp.MustCompile(`^(?:rod[ae]r?|roda|execut[ae]r?|executa|inici[ae]r?|inicia|sub[ai]r?|sobe|lig[ae]r?|liga|ligue|(?:d[ae] )?play(?: n[ao])?|d[ae] o play(?: n[ao])?)(?: (?:a|o))?(?: projeto)?(?: (?:de|da|do))? (?P<alvo>.+)$`)},
	{Parar, regexp.MustCompile(`^(?:par[ae]r?|para|derrub[ae]r?|derruba|deslig[ae]r?|desliga|desligue)(?: (?:a|o))?(?: projeto)?(?: (?:de|da|do))? (?P<alvo>.+)$`)},
	{Sim, regexp.MustCompile(`^(?:sim|pode|pode sim|sim pode|confirmo|confirma|confirmado|isso|isso mesmo|claro|pode ir|manda|manda ver|ok|beleza|certo|fechado)$`)},
	{Nao, regexp.MustCompile(`^(?:nao|nao nao|cancela|cancelar|cancele|deixa|deixa pra la|esquece|nao quero|nao precisa|para|pare)$`)},
}

// verbos são as formas que começam um pedido. Uma palavra a até duas letras
// de distância de uma delas (a transcrição troca "rodar" por "codar" ou
// "cudar") vale por ela, mas com dúvida: a ação pergunta antes.
var verbos = []string{"rodar", "rode", "executar", "execute", "iniciar", "parar", "derrubar", "desligar", "desligue",
	"abrir", "mostrar", "encerrar", "encerra", "encerre", "concluir", "conclua", "finalizar", "terminar", "termina"}

// corrigirVerbo troca a primeira palavra pelo verbo mais perto dela (só para
// palavras de 5 letras ou mais, que não são um verbo, e um verbo só no mais
// perto) e diz se trocou.
func corrigirVerbo(t string) (string, bool) {
	primeira, resto, _ := strings.Cut(t, " ")
	if len([]rune(primeira)) < 5 {
		return t, false
	}
	for _, v := range verbos {
		if v == primeira {
			return t, false
		}
	}
	melhor, empate, menor := "", false, 3
	for _, v := range verbos {
		d := distancia([]rune(primeira), []rune(v))
		switch {
		case d < menor:
			melhor, empate, menor = v, false, d
		case d == menor:
			empate = true
		}
	}
	if melhor == "" || empate {
		return t, false
	}
	return strings.TrimSpace(melhor + " " + resto), true
}

// tirarNome tira do começo o "ei" e o nome da abelha, mesmo transcrito
// com uma ou duas letras trocadas ("com meia", "colmeias"), juntando até
// duas palavras: ouvindo o nome, a fala vem com ele.
func tirarNome(t, nome string) string {
	t = strings.TrimSpace(strings.TrimPrefix(t, "ei "))
	if nome == "" {
		return t
	}
	palavras := strings.Fields(t)
	junto := strings.ReplaceAll(nome, " ", "")
	tolerancia := 0
	if len([]rune(junto)) >= 5 {
		tolerancia = 2
	}
	for n := 1; n <= min(2, len(palavras)); n++ {
		inicio := strings.Join(palavras[:n], "")
		if distancia([]rune(inicio), []rune(junto)) <= tolerancia {
			return strings.Join(palavras[n:], " ")
		}
	}
	return t
}

// Entender lê a frase transcrita. O nome da abelha no começo ("Colmeia,
// rodar a loja web") e as cortesias saem antes.
func Entender(frase, nomeAbelha string) Pedido {
	original := normalizar(frase)
	t := tirarNome(original, normalizar(nomeAbelha))
	if t == "" && original != "" {
		return Pedido{Intencao: SoNome}
	}
	t = fimSobrando.ReplaceAllString(inicioSobrando.ReplaceAllString(t, ""), "")
	if regras[0].padrao.MatchString(t) {
		return Pedido{Intencao: OQueEspera}
	}
	t, duvida := corrigirVerbo(strings.TrimSpace(t))
	for _, r := range regras {
		m := r.padrao.FindStringSubmatch(t)
		if m == nil {
			continue
		}
		p := Pedido{Intencao: r.intencao, Duvida: duvida}
		if i := r.padrao.SubexpIndex("alvo"); i > 0 {
			p.Alvo = strings.TrimSpace(m[i])
			if p.Alvo == "" {
				continue
			}
		}
		return p
	}
	return Pedido{}
}
