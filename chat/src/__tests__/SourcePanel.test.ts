import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import SourcePanel from '../components/SourcePanel.vue'

describe('SourcePanel', () => {
  const sources = [
    { content: 'Low relevance source', score: 0.3, memory_id: 'mem-low' },
    { content: 'High relevance source', score: 0.9, memory_id: 'mem-high' },
    { content: 'Medium relevance source', score: 0.6, memory_id: 'mem-mid' },
  ]

  it('renders all sources', () => {
    const wrapper = mount(SourcePanel, { props: { sources } })
    const items = wrapper.findAll('.source-item')
    expect(items).toHaveLength(3)
  })

  it('sorts sources by score descending', () => {
    const wrapper = mount(SourcePanel, { props: { sources } })
    const badges = wrapper.findAll('.score-badge')
    expect(badges[0].text()).toBe('90%')
    expect(badges[1].text()).toBe('60%')
    expect(badges[2].text()).toBe('30%')
  })

  it('displays score as percentage', () => {
    const wrapper = mount(SourcePanel, {
      props: { sources: [{ content: 'x', score: 0.75, memory_id: 'id' }] },
    })
    expect(wrapper.find('.score-badge').text()).toBe('75%')
  })

  it('displays memory ID', () => {
    const wrapper = mount(SourcePanel, {
      props: { sources: [{ content: 'x', score: 0.5, memory_id: 'abc-123' }] },
    })
    expect(wrapper.find('.source-id').text()).toContain('abc-123')
  })

  it('truncates long content at 200 chars', () => {
    const longContent = 'A'.repeat(250)
    const wrapper = mount(SourcePanel, {
      props: { sources: [{ content: longContent, score: 0.5, memory_id: 'id' }] },
    })
    const text = wrapper.find('.source-content').text()
    expect(text).toHaveLength(203) // 200 + '...'
    expect(text.endsWith('...')).toBe(true)
  })

  it('does not truncate short content', () => {
    const wrapper = mount(SourcePanel, {
      props: { sources: [{ content: 'Short text', score: 0.5, memory_id: 'id' }] },
    })
    expect(wrapper.find('.source-content').text()).toBe('Short text')
  })

  it('applies green color for high scores (>=0.8)', () => {
    const wrapper = mount(SourcePanel, {
      props: { sources: [{ content: 'x', score: 0.9, memory_id: 'id' }] },
    })
    const badge = wrapper.find('.score-badge')
    expect(badge.attributes('style')).toContain('#00ff88')
  })

  it('applies cyan color for medium scores (0.5-0.8)', () => {
    const wrapper = mount(SourcePanel, {
      props: { sources: [{ content: 'x', score: 0.6, memory_id: 'id' }] },
    })
    const badge = wrapper.find('.score-badge')
    expect(badge.attributes('style')).toContain('#00d4ff')
  })

  it('applies gray color for low scores (<0.5)', () => {
    const wrapper = mount(SourcePanel, {
      props: { sources: [{ content: 'x', score: 0.3, memory_id: 'id' }] },
    })
    const badge = wrapper.find('.score-badge')
    expect(badge.attributes('style')).toContain('#8b949e')
  })
})
