// Package atualizacao procura uma versão nova da Colmeia nas releases do
// GitHub, baixa, confere e troca os programas. Nada é instalado sem a
// assinatura da chave da Colmeia: o manifesto da release (versão, o que
// mudou e o SHA-256 de cada pacote) é assinado com Ed25519 e só a chave
// pública vem aqui dentro. Um manifesto velho, mesmo assinado, não serve:
// só vale uma versão maior que a que está rodando.
package atualizacao

import (
	"crypto/ed25519"
	"encoding/base64"
	"encoding/json"
	"errors"
	"regexp"
	"strconv"
	"strings"
)

// ChavePublica confere as releases. A privada fica só com quem publica (o
// segredo COLMEIA_ASSINATURA do GitHub e a cópia guardada fora do repositório).
// É var só para o teste de ponta a ponta compilar com uma chave de teste (-X).
var ChavePublica = "hT1X0p0FGAmdZZPwzplnOp9lkXzX3YNt0XmKvrguV8k="

// O que a assinatura cobre começa com este rótulo, para ela não servir em
// outro lugar.
const rotulo = "colmeia-atualizacao\n"

const (
	NomeManifesto  = "colmeia.json"
	NomeAssinatura = "colmeia.json.sig"
)

// Limites de tamanho do que vem da rede antes de ser conferido.
const (
	limiteManifesto = 256 << 10
	limiteNotas     = 64 << 10
)

// Manifesto é o colmeia.json de uma release.
type Manifesto struct {
	Versao   string             `json:"versao"`
	Notas    string             `json:"notas"`
	Arquivos map[string]Arquivo `json:"arquivos"`
}

type Arquivo struct {
	SHA256  string `json:"sha256"`
	Tamanho int64  `json:"tamanho"`
}

var (
	formatoVersao = regexp.MustCompile(`^\d{1,4}\.\d{1,4}\.\d{1,4}$`)
	formatoHash   = regexp.MustCompile(`^[0-9a-f]{64}$`)
	formatoNome   = regexp.MustCompile(`^[a-z0-9][a-z0-9._-]{0,80}$`)
)

var ErrAssinatura = errors.New("A versão nova não tem a assinatura da Colmeia; não instalei.")

// Assinar devolve a assinatura (base64) do manifesto.
func Assinar(privada ed25519.PrivateKey, manifesto []byte) string {
	return base64.StdEncoding.EncodeToString(ed25519.Sign(privada, append([]byte(rotulo), manifesto...)))
}

// Verificar confere a assinatura com a chave e só então lê o manifesto.
func Verificar(chave string, manifesto []byte, assinatura string) (Manifesto, error) {
	var m Manifesto
	pub, err := base64.StdEncoding.DecodeString(chave)
	if err != nil || len(pub) != ed25519.PublicKeySize {
		return m, errors.New("chave pública inválida")
	}
	sig, err := base64.StdEncoding.DecodeString(strings.TrimSpace(assinatura))
	if err != nil || !ed25519.Verify(pub, append([]byte(rotulo), manifesto...), sig) {
		return m, ErrAssinatura
	}
	if err := json.Unmarshal(manifesto, &m); err != nil {
		return m, errors.New("O manifesto da versão nova não abriu.")
	}
	if !formatoVersao.MatchString(m.Versao) || len(m.Notas) > limiteNotas {
		return m, errors.New("O manifesto da versão nova está fora do formato.")
	}
	for nome, a := range m.Arquivos {
		if !formatoNome.MatchString(nome) || !formatoHash.MatchString(a.SHA256) || a.Tamanho <= 0 {
			return m, errors.New("O manifesto da versão nova está fora do formato.")
		}
	}
	return m, nil
}

// MaisNova diz se a versão a vem depois de b ("0.10.0" depois de "0.9.3").
func MaisNova(a, b string) bool {
	pa, pb := partes(a), partes(b)
	if pa == nil || pb == nil {
		return false
	}
	for i := range 3 {
		if pa[i] != pb[i] {
			return pa[i] > pb[i]
		}
	}
	return false
}

func partes(v string) []int {
	v = strings.TrimPrefix(v, "v")
	if !formatoVersao.MatchString(v) {
		return nil
	}
	p := make([]int, 3)
	for i, s := range strings.Split(v, ".") {
		p[i], _ = strconv.Atoi(s)
	}
	return p
}

// NotasDoChangelog tira do CHANGELOG.md a seção "## [versao]" (sem o título),
// até a próxima seção de versão.
func NotasDoChangelog(changelog, versao string) (string, bool) {
	var notas []string
	achou := false
	for _, linha := range strings.Split(changelog, "\n") {
		if strings.HasPrefix(linha, "## ") {
			if achou {
				break
			}
			achou = strings.HasPrefix(linha, "## ["+versao+"]")
			continue
		}
		if achou {
			notas = append(notas, linha)
		}
	}
	return strings.TrimSpace(strings.Join(notas, "\n")), achou
}
