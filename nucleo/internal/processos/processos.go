// Package processos descobre se uma ferramenta já está rodando numa pasta fora
// da Colmeia (no IntelliJ, num terminal), para avisar antes de abrir a mesma
// conversa em dois lugares.
package processos

// Marca é a variável que a Colmeia põe nos agentes que ela abre; processos com
// ela não contam como "fora da Colmeia".
const Marca = "COLMEIA_AGENTE"
