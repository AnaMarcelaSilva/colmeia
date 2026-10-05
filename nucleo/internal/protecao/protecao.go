// Package protecao deixa as pastas da Colmeia só para o usuário atual.
//
// No Linux e no macOS isso é a permissão 0700, que quem cria a pasta já
// aplica. No Windows as permissões são uma lista de acesso (ACL): a pasta
// recebe uma lista protegida, que não herda a da pasta de cima, só com o
// usuário e o sistema.
package protecao
