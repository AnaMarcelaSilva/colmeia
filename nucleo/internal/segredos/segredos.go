// Package segredos reconhece textos que parecem conter uma senha, um token
// ou uma chave. Serve para o núcleo não guardar esses textos (o histórico de
// mensagens, por exemplo). É uma leitura, não uma garantia: um segredo fora
// dos formatos conhecidos passa, e por isso o que fica guardado tem limite e
// pode ser apagado.
package segredos

import (
	"regexp"
	"strings"
	"unicode"
)

// Formatos conhecidos. Os prefixos de chave exigem um tamanho mínimo depois
// deles, para "sk-learn" ou "risk-free" não contarem.
var padroes = []*regexp.Regexp{
	// Chaves de API com prefixo: Anthropic, OpenAI e parecidas.
	regexp.MustCompile(`\bsk-ant-[A-Za-z0-9_-]{16,}`),
	regexp.MustCompile(`\bsk-[A-Za-z0-9_-]{20,}`),
	// GitHub (ghp_, gho_, ghu_, ghs_, ghr_) e os tokens de granularidade fina.
	regexp.MustCompile(`\bgh[pousr]_[A-Za-z0-9]{20,}`),
	regexp.MustCompile(`\bgithub_pat_[A-Za-z0-9_]{20,}`),
	// GitLab e Slack.
	regexp.MustCompile(`\bglpat-[A-Za-z0-9_-]{16,}`),
	regexp.MustCompile(`\bxox[baprs]-[A-Za-z0-9-]{10,}`),
	// Chave de acesso da AWS.
	regexp.MustCompile(`\bAKIA[0-9A-Z]{16}\b`),
	// Chave privada colada inteira.
	regexp.MustCompile(`-----BEGIN [A-Z0-9 ]*PRIVATE KEY`),
	// JWT: cabeçalho e conteúdo em base64url começando por {".
	regexp.MustCompile(`\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.`),
	// "senha: xyz123", "password=...", "token = ...", "API_KEY=...": um nome
	// de segredo seguido de : ou = e de um valor sem espaço.
	regexp.MustCompile(`(?i)\b(senha|password|passwd|pwd|token|secret|segredo|api[_-]?key|client[_-]?secret)["']?\s*[:=]\s*["']?[^\s"']{6,}`),
}

// Formatos mais soltos, que só contam se o valor tiver um algarismo ou um
// símbolo (uma senha de verdade quase sempre tem; uma frase comum, não):
//   - um nome com senha, password, secret, token ou key em qualquer posição
//     ("db_password=…", "SENHA_DB=…", "minhasenha: …") seguido de : ou =;
//   - a frase "a senha (do banco) é …".
var soltos = []*regexp.Regexp{
	regexp.MustCompile(`(?i)[\p{L}0-9_.-]*(?:senha|password|passwd|pwd|secret|segredo|token|key|chave)[\p{L}0-9_.-]*["']?[ \t]*[:=][ \t]*["']?([^\s"']{6,})`),
	regexp.MustCompile(`(?i)\b(?:senha|password)\b[^\n.:=]{0,40}?\s(?:é|eh|is)[ \t]*:?[ \t]*["']?([^\s"']{6,})`),
}

// Parece diz se o texto tem algo no formato de um segredo.
func Parece(texto string) bool {
	for _, p := range padroes {
		if p.MatchString(texto) {
			return true
		}
	}
	for _, p := range soltos {
		for _, m := range p.FindAllStringSubmatch(texto, -1) {
			if valorDeSenha(m[1]) {
				return true
			}
		}
	}
	return false
}

// valorDeSenha: o valor tem um algarismo ou um símbolo (não só letras).
func valorDeSenha(v string) bool {
	v = strings.TrimRight(v, ".,;)")
	if len([]rune(v)) < 6 {
		return false
	}
	return strings.IndexFunc(v, func(r rune) bool { return !unicode.IsLetter(r) }) >= 0
}
