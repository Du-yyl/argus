/**
 * Time:2024/12/14 12:39 58
 * Name:command.type.ts
 * Path:src/types
 * ProjectName:argus-src
 * Author:charlatans
 *
 *  Il n'ya qu'un héroïsme au monde :
 *     c'est de voir le monde tel qu'il est et de l'aimer.
 */
export type CommandType = {
  // 指令名称 如：greet
  name: string
  // 指令描述
  description: string
  // 具体参数
  params: [
    {
      // 参数名称【该名臣是传递到后端的参数名称，如：name ，传递后后端时将使用 name: xxx】
      name: string
      // 提示字段 如：【输入名称】
      label: string
      type: 'text' | 'checkbox'
      // 默认值
      value: string | boolean
      // 提示字段，如：【输入名称】
      placeholder: string
    }
  ]
  // 返回结果
  result: null | string | number | boolean | object
}
