package com.agupta07505.smartisland.coucou.notas

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class NotasArranqueTest {
    @Test fun theClassOnNowWins() =
        assertEquals("FISICA I", NotasArranque.materiaPara(ClaseAhora("FISICA I", null, null), "BIOLOGIA I"))

    @Test fun fallsBackToTheLastSubject() =
        assertEquals("BIOLOGIA I", NotasArranque.materiaPara(null, "BIOLOGIA I"))

    @Test fun aBlankClassDoesNotWin() =
        assertEquals("BIOLOGIA I", NotasArranque.materiaPara(ClaseAhora(" ", "10:00", null), "BIOLOGIA I"))

    @Test fun aBlankLastSubjectIsNotASubject() =
        assertNull(NotasArranque.materiaPara(null, " "))

    @Test fun nothingToRecord() =
        assertNull(NotasArranque.materiaPara(null, null))
}
