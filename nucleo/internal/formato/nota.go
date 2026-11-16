// Package formato guarda o estilo da reunião: como a pessoa escreve a daily e
// a sprint quando escreve à mão, dito de um jeito que o agente consegue
// seguir. É o contrato entre o que os agentes gravam e o que a tela, o slide
// e a lousa da reunião mostram. Lêem daqui o agente da tarefa (as instruções
// e a descrição de complementar_nota no servidor MCP e o pedido da daily que
// o núcleo põe na fila quando a tarefa vai para Revisão ou Concluído), a
// proposta da sprint e o montador da lousa da reunião.
package formato

// MaxNotaDeConclusao: acima disto, a nota de conclusão está relatando em vez
// de resumir (o campo continua aceitando até 4.000).
const MaxNotaDeConclusao = 900

// MaxLinhasDaNota é o tamanho em linhas de uma nota de conclusão.
const MaxLinhasDaNota = 12

// Estilo é como a pessoa escreve para a reunião, nas palavras que ela usa.
// Quem lê é o time de desenvolvimento e a gerência; ela fala a partir do
// texto, por cliente, sistema ou projeto, uma tarefa de cada vez.
const Estilo = "Quem lê é o time de desenvolvimento e a gerência, na daily ou na reunião de sprint. A pessoa fala a partir do que você escreve, por cliente, sistema ou projeto, uma tarefa de cada vez. Escreva como ela escreve.\n" +
	"1. Cada item começa pelo substantivo do que foi feito (Correção, Ajuste, Configuração, Validação, Tela nova, Integração), seguido do que mudou. Quando foi um erro, vem o sintoma que a pessoa via e depois a causa, nessa ordem, e o que passou a valer: \"Correção da transferência de comanda, que não aparecia nas opções da mesa\"; \"Dava o erro \"Não foi possível verificar os dados\". Acontecia porque a validação só olhava as permissões da loja e não as do painel. Agora considera as duas.\"\n" +
	"2. O que é novo é dito como novo, com o nome técnico e a tecnologia, porque o time precisa saber o que passou a existir: \"coluna nova codigo_etiqueta no item da comanda, para saber que o item veio da balança\", \"endpoint novo /lojas/endereco na API em Go\", \"tabela nova de acessos das lojas\", \"tela nova em React no painel\", \"campo novo no instalador\". Vale para coluna, tabela, endpoint, campo, parâmetro, tela, serviço e integração, e para o que protege o quê e por quê (\"o registro fica protegido pelo token do primeiro cadastro, então só a própria loja troca o endereço\"). O que fica de fora é o que ninguém fala em reunião: nome de arquivo, função, classe, commit, branch, comando, caminho, trecho de código ou SQL.\n" +
	"3. Uma integração ou um fluxo com mais de um passo vai numerado, na ordem em que acontece, um passo por linha, dizendo quem faz o quê: \"1. No instalador, um campo novo para o endereço público da loja. 2. Quando a API sobe, ela manda o endereço para a central junto com o CNPJ e o token da loja. 3. A central confere o token e grava o endereço.\"\n" +
	"4. Quando o cliente pediu ou reclamou, o contexto vem antes do que foi feito: \"O cliente contou que já usa há dois dias e está gostando\"; \"Pendente do cliente: lançar vários itens na mesma rodada, porque hoje precisa enviar e adicionar de novo\".\n" +
	"5. Por último o estado: como foi testado, com quem foi validado, o que falta e de quem depende: \"Testado de ponta a ponta pelo endereço real. Falta subir para revisão e decidir com o time onde a gestão web fica hospedada.\"\n" +
	"6. Frases curtas e diretas, no presente ou no passado simples. Nada de \"com sucesso\", \"foi implementado\", \"conforme solicitado\", \"de forma\", nem elogio ao próprio trabalho, nem repetir o título da tarefa. Nada de adjetivos vazios.\n" +
	"7. Para o slide, cada tarefa tem um tópico de duas a quatro palavras, o nome da coisa (\"Trava da comanda\", \"Código de barras\", \"Endereço da loja no painel\"), e uma frase para quem não é técnico dizendo o que passou a valer para quem usa (\"O código lido com zeros à esquerda é reconhecido no produto.\").\n" +
	"8. Imagens: a tela depois da mudança quando há uma tela (página no navegador, app com interface), a tela do erro quando a tarefa foi um erro visível, e um diagrama só quando a tarefa é uma integração. Nunca terminal, código, log, teste ou diff. No máximo duas por tarefa."

// NotaDaily é o formato da nota, para a descrição de complementar_nota.
const NotaDaily = "A nota da daily é o que o usuário lê na reunião, num slide por projeto e na lousa da reunião, sem ver o seu terminal. " +
	"Primeira linha, uma frase para quem não é técnico dizendo o que mudou para quem usa e o que passou a valer. " +
	"Depois, se mudou mais de uma coisa, um item por mudança, cada um numa linha, começando pelo substantivo (Correção, Validação, Tela nova). Até 6 itens. Um fluxo novo com mais de um passo vai numerado. " +
	"Quando a tarefa foi um erro, um item com o sintoma, a causa (acontecia porque) e o que passou a valer (agora). " +
	"O que é novo é dito como novo, com o nome técnico e a tecnologia, porque o time precisa saber o que passou a existir: coluna nova codigo_etiqueta no item da comanda, endpoint novo /lojas/endereco na API em Go, tela nova em React no painel. " +
	"Nunca cite arquivo, função, classe, commit, branch, comando, caminho, trecho de código ou SQL; esses ninguém fala em reunião. " +
	"Por último, se faltar algo, um item começando com Falta, dizendo o que falta e de quem depende. " +
	"Não escreva com sucesso, foi implementado, conforme solicitado, nem repita o título da tarefa. " +
	"Até 900 caracteres e 12 linhas ao concluir. Markdown simples, só listas e negrito. " +
	"Anexe uma imagem só quando há uma tela para mostrar depois da mudança (uma página no navegador da tarefa ou um app com interface) ou a tela do erro corrigido, no máximo duas. Nunca de terminal, código, log, teste ou diff. " +
	"Não escreva na lousa da tarefa ao concluir, a não ser que o usuário peça um desenho; a lousa da reunião é montada pelo núcleo a partir da nota."

// ExemploNotaDaily é uma nota no formato, com um projeto fictício.
const ExemploNotaDaily = "A gestão web de cada loja passa a abrir pelo endereço certo a partir do painel.\n" +
	"1. Campo novo no instalador para o endereço público da loja.\n" +
	"2. Quando a API sobe, ou quando a aba de configuração é salva, ela manda o endereço para a central junto com o CNPJ e o token da loja.\n" +
	"3. A central confere o token e grava o endereço. O registro fica protegido pelo token do primeiro cadastro, então só a própria loja troca o endereço dela.\n" +
	"4. No painel, Gestão web abre direto com uma loja e pede a escolha com várias.\n" +
	"- Testado de ponta a ponta pelo endereço real.\n" +
	"- Falta decidir com o time se a gestão web fica no mesmo endereço do serviço."

// AoTerminar é a frase das instruções gerais do servidor MCP.
const AoTerminar = "Ao terminar o trabalho, antes de encerrar, grave a nota da daily com complementar_nota no formato da reunião (veja a descrição de complementar_nota). "

// PedidoDaily é o pedido que entra no terminal quando a tarefa vai para
// Revisão ou Concluído com a nota vazia; `onde` é "Revisão" ou "Concluído".
func PedidoDaily(onde string) string {
	return "A tarefa foi para " + onde + ". Grave a nota da daily para a reunião, no formato da descrição de complementar_nota. " +
		"A primeira linha diz o que mudou para quem usa, depois um item por mudança começando pelo substantivo (um fluxo novo vai numerado), e por último o que falta e de quem depende, se faltar algo. " +
		"Quando foi um erro, diga o sintoma, a causa e o que passou a valer. O que é novo (coluna, tabela, endpoint, campo, tela) é dito como novo, com o nome técnico e a tecnologia: o time precisa saber o que passou a existir. " +
		"Sem nomes de arquivo, função, commit, branch, caminho ou SQL. Anexe uma captura só se houver uma tela para mostrar."
}
