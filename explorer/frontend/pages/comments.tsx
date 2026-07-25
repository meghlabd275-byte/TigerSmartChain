/**
 * Comments & Notes Page
 * Complete frontend for address annotations and notes
 */

import { useState, useEffect } from 'react'
import Head from 'next/head'
import Link from 'next/link'

interface Comment {
  id: number
  address: string
  tx_hash?: string
  user_id: number
  username: string
  content: string
  is_private: boolean
  reactions: number
  created_at: string
}

interface Note {
  id: number
  address: string
  title: string
  content: string
  color: string
  created_at: string
}

export default function CommentsPage() {
  const [address, setAddress] = useState('')
  const [comments, setComments] = useState<Comment[]>([])
  const [notes, setNotes] = useState<Note[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [activeTab, setActiveTab] = useState<'comments' | 'notes'>('comments')
  
  const [newComment, setNewComment] = useState({
    content: '',
    is_private: false
  })
  const [newNote, setNewNote] = useState({
    title: '',
    content: '',
    color: '#3B82F6'
  })

  useEffect(() => {
    if (address) {
      fetchComments()
      fetchNotes()
    }
  }, [address])

  async function fetchComments() {
    try {
      const res = await fetch(`/api/v1/comments/address/${address}`)
      if (res.ok) {
        const data = await res.json()
        setComments(data.comments || [])
      }
    } catch (err: any) {
      console.error('Failed to fetch comments:', err)
    }
  }

  async function fetchNotes() {
    try {
      const res = await fetch(`/api/v1/notes/address/${address}`)
      if (res.ok) {
        const data = await res.json()
        setNotes(data.notes || [])
      }
    } catch (err: any) {
      console.error('Failed to fetch notes:', err)
    }
  }

  async function createComment() {
    if (!newComment.content.trim()) return
    
    setLoading(true)
    try {
      const res = await fetch('/api/v1/comments', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: address,
          content: newComment.content,
          is_private: newComment.is_private
        })
      })
      if (res.ok) {
        await fetchComments()
        setNewComment({ content: '', is_private: false })
      }
    } catch (err: any) {
      setError(err.message)
    } finally {
      setLoading(false)
    }
  }

  async function deleteComment(id: number) {
    try {
      const res = await fetch(`/api/v1/comments/${id}`, { method: 'DELETE' })
      if (res.ok) {
        await fetchComments()
      }
    } catch (err: any) {
      setError(err.message)
    }
  }

  async function createNote() {
    if (!newNote.title.trim()) return
    
    setLoading(true)
    try {
      const res = await fetch('/api/v1/notes', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          address: address,
          title: newNote.title,
          content: newNote.content,
          color: newNote.color
        })
      })
      if (res.ok) {
        await fetchNotes()
        setNewNote({ title: '', content: '', color: '#3B82F6' })
      }
    } catch (err: any) {
      setError(err.message)
    } finally {
      setLoading(false)
    }
  }

  async function deleteNote(id: number) {
    try {
      const res = await fetch(`/api/v1/notes/${id}`, { method: 'DELETE' })
      if (res.ok) {
        await fetchNotes()
      }
    } catch (err: any) {
      setError(err.message)
    }
  }

  function formatDate(dateStr: string): string {
    const date = new Date(dateStr)
    return date.toLocaleDateString() + ' ' + date.toLocaleTimeString()
  }

  function getColorStyle(color: string): string {
    return {
      backgroundColor: color + '20',
      borderLeftColor: color
    } as React.CSSProperties
  }

  return (
    <>
      <Head>
        <title>Comments & Notes | TigerScan</title>
      </Head>
      
      <div className="min-h-screen bg-gray-50">
        <header className="bg-white shadow">
          <div className="max-w-7xl mx-auto py-6 px-4 sm:px-6 lg:px-8">
            <div className="flex items-center justify-between">
              <h1 className="text-3xl font-bold text-gray-900">
                Comments & Notes
              </h1>
              <Link href="/" className="text-blue-600 hover:text-blue-800">
                ← Back to Home
              </Link>
            </div>
          </div>
        </header>

        <main className="max-w-7xl mx-auto py-6 px-4 sm:px-6 lg:px-8">
          {/* Search */}
          <div className="mb-8">
            <label className="block text-sm font-medium text-gray-700 mb-2">
              Address
            </label>
            <input
              type="text"
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              placeholder="0x..."
              className="w-full px-4 py-2 border border-gray-300 rounded-md focus:ring-2 focus:ring-blue-500"
            />
          </div>

          {error && (
            <div className="mb-4 p-4 bg-red-100 border border-red-400 text-red-700 rounded">
              {error}
              <button onClick={() => setError(null)} className="ml-4 text-sm underline">
                Dismiss
              </button>
            </div>
          )}

          {/* Tabs */}
          <div className="border-b border-gray-200 mb-6">
            <nav className="-mb-px flex space-x-8">
              {[
                { id: 'comments', label: 'Comments', count: comments.length },
                { id: 'notes', label: 'Private Notes', count: notes.length }
              ].map((tab) => (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id as any)}
                  className={`py-4 px-1 border-b-2 font-medium text-sm ${
                    activeTab === tab.id
                      ? 'border-blue-500 text-blue-600'
                      : 'border-transparent text-gray-500 hover:text-gray-700'
                  }`}
                >
                  {tab.label}
                  <span className="ml-2 px-2 py-0.5 text-xs rounded-full bg-gray-100">
                    {tab.count}
                  </span>
                </button>
              ))}
            </nav>
          </div>

          {/* Comments Tab */}
          {activeTab === 'comments' && address && (
            <div>
              {/* Create Comment */}
              <div className="bg-white p-6 rounded-lg shadow mb-6">
                <h3 className="text-lg font-medium mb-4">Add Comment</h3>
                <div className="space-y-4">
                  <textarea
                    value={newComment.content}
                    onChange={(e) => setNewComment({...newComment, content: e.target.value})}
                    placeholder="Write your comment..."
                    rows={3}
                    className="w-full px-4 py-2 border border-gray-300 rounded-md"
                  />
                  <div className="flex items-center justify-between">
                    <label className="flex items-center">
                      <input
                        type="checkbox"
                        checked={newComment.is_private}
                        onChange={(e) => setNewComment({...newComment, is_private: e.target.checked})}
                        className="mr-2"
                      />
                      <span className="text-sm text-gray-600">Private comment</span>
                    </label>
                    <button
                      onClick={createComment}
                      disabled={loading || !newComment.content.trim()}
                      className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50"
                    >
                      Post Comment
                    </button>
                  </div>
                </div>
              </div>

              {/* Comments List */}
              <div className="space-y-4">
                {comments.length > 0 ? (
                  comments.map((comment) => (
                    <div key={comment.id} className="bg-white p-6 rounded-lg shadow">
                      <div className="flex items-start justify-between">
                        <div className="flex items-center gap-3">
                          <div className="h-10 w-10 rounded-full bg-blue-100 flex items-center justify-center">
                            <span className="text-blue-600 font-medium">
                              {comment.username.charAt(0).toUpperCase()}
                            </span>
                          </div>
                          <div>
                            <div className="font-medium text-gray-900">
                              {comment.username}
                            </div>
                            <div className="text-sm text-gray-500">
                              {formatDate(comment.created_at)}
                              {comment.is_private && (
                                <span className="ml-2 px-2 py-0.5 text-xs bg-gray-100 rounded">
                                  Private
                                </span>
                              )}
                            </div>
                          </div>
                        </div>
                        <button
                          onClick={() => deleteComment(comment.id)}
                          className="text-gray-400 hover:text-red-600"
                        >
                          ✕
                        </button>
                      </div>
                      <div className="mt-4 text-gray-700">
                        {comment.content}
                      </div>
                      <div className="mt-4 flex items-center gap-4 text-sm text-gray-500">
                        <button className="hover:text-blue-600">
                          👍 {comment.reactions}
                        </button>
                        <button className="hover:text-blue-600">
                          Reply
                        </button>
                      </div>
                    </div>
                  ))
                ) : (
                  <div className="bg-white p-6 rounded-lg shadow text-center text-gray-500">
                    No comments yet. Be the first to comment!
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Notes Tab */}
          {activeTab === 'notes' && address && (
            <div>
              {/* Create Note */}
              <div className="bg-white p-6 rounded-lg shadow mb-6">
                <h3 className="text-lg font-medium mb-4">Add Private Note</h3>
                <div className="space-y-4">
                  <input
                    type="text"
                    value={newNote.title}
                    onChange={(e) => setNewNote({...newNote, title: e.target.value})}
                    placeholder="Note title"
                    className="w-full px-4 py-2 border border-gray-300 rounded-md"
                  />
                  <textarea
                    value={newNote.content}
                    onChange={(e) => setNewNote({...newNote, content: e.target.value})}
                    placeholder="Note content..."
                    rows={3}
                    className="w-full px-4 py-2 border border-gray-300 rounded-md"
                  />
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="text-sm text-gray-600">Color:</span>
                      {['#3B82F6', '#10B981', '#F59E0B', '#EF4444', '#8B5CF6'].map((color) => (
                        <button
                          key={color}
                          onClick={() => setNewNote({...newNote, color})}
                          className={`w-6 h-6 rounded-full ${
                            newNote.color === color ? 'ring-2 ring-offset-2 ring-gray-400' : ''
                          }`}
                          style={{ backgroundColor: color }}
                        />
                      ))}
                    </div>
                    <button
                      onClick={createNote}
                      disabled={loading || !newNote.title.trim()}
                      className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50"
                    >
                      Save Note
                    </button>
                  </div>
                </div>
              </div>

              {/* Notes List */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {notes.length > 0 ? (
                  notes.map((note) => (
                    <div
                      key={note.id}
                      className="bg-white p-4 rounded-lg shadow"
                      style={getColorStyle(note.color)}
                    >
                      <div className="flex items-start justify-between">
                        <div>
                          <h4 className="font-medium text-gray-900">{note.title}</h4>
                          <p className="text-sm text-gray-600 mt-1">{note.content}</p>
                          <p className="text-xs text-gray-500 mt-2">
                            {formatDate(note.created_at)}
                          </p>
                        </div>
                        <button
                          onClick={() => deleteNote(note.id)}
                          className="text-gray-400 hover:text-red-600"
                        >
                          ✕
                        </button>
                      </div>
                    </div>
                  ))
                ) : (
                  <div className="col-span-2 bg-white p-6 rounded-lg shadow text-center text-gray-500">
                    No private notes yet. Create one to keep track of important information!
                  </div>
                )}
              </div>
            </div>
          )}

          {!address && (
            <div className="text-center py-12 text-gray-500">
              Enter an address to view or add comments and notes
            </div>
          )}
        </main>
      </div>
    </>
  )
}
