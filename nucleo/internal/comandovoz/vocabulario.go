package comandovoz

import "strings"

// LimiteVocabulario é o tamanho máximo do vocabulário (em bytes): o Whisper
// só aproveita uns 220 tokens do prompt, e o colmeia-voz recusa mais que 800.
const LimiteVocabulario = 800

// Vocabulario monta o prompt inicial do Whisper para o perfil: frases no
// jeito dos comandos, com o nome da abelha, "Colmeia" e os nomes dos
// projetos e das tarefas, para que ele escreva esses nomes em vez de algo
// que soa parecido ("go meia" no lugar de "Colmeia"). As tarefas abertas vêm
// antes das concluídas; o que não cabe no limite fica de fora inteiro.
func Vocabulario(nomeAbelha string, q Quadro) string {
	nome := strings.TrimSpace(nomeAbelha)
	if nome == "" {
		nome = "Colmeia"
	}
	var b strings.Builder
	cabe := func(s string) bool {
		if b.Len()+len(s) > LimiteVocabulario {
			return false
		}
		b.WriteString(s)
		return true
	}
	cabe(nome + ", rodar o projeto")
	if !strings.EqualFold(nome, "Colmeia") {
		cabe(" na Colmeia")
	}
	cabe(". Parar, abrir a tarefa, encerrar a tarefa, como está a tarefa? O que espera por mim? Sim. Não.")

	projetos := make([]string, 0, len(q.Projetos))
	for _, p := range q.Projetos {
		projetos = append(projetos, p.Nome)
		for _, e := range p.Execucoes {
			projetos = append(projetos, e.Nome)
		}
	}
	itens(cabe, " Projetos: ", projetos)

	tarefas := make([]string, 0, len(q.Tarefas))
	for _, concluidas := range []bool{false, true} {
		for _, t := range q.Tarefas {
			if (t.Coluna == "concluido") == concluidas {
				tarefas = append(tarefas, t.Titulo)
			}
		}
	}
	itens(cabe, " Tarefas: ", tarefas)
	return b.String()
}

// itens escreve "rótulo: a, b, c." com os que couberem e sem repetir.
func itens(cabe func(string) bool, rotulo string, nomes []string) {
	vistos := map[string]bool{}
	primeiro := true
	for _, n := range nomes {
		n = strings.Join(strings.Fields(n), " ")
		chave := strings.ToLower(n)
		if n == "" || vistos[chave] {
			continue
		}
		vistos[chave] = true
		antes := ", "
		if primeiro {
			antes = rotulo
		}
		// Sempre sobra lugar para o ponto final.
		if !cabe(antes + n) {
			break
		}
		primeiro = false
	}
	if !primeiro {
		cabe(".")
	}
}
