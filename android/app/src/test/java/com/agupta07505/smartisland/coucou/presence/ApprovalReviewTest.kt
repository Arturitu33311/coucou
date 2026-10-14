package com.agupta07505.smartisland.coucou.presence

import com.agupta07505.smartisland.coucou.ui.ApprovalReview
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** A command is not allowed from the phone without having been seen. */
class ApprovalReviewTest {
    @Test
    fun a_command_the_laptop_had_to_cut_is_not_allowed_from_the_phone() {
        val cut = "a".repeat(299) + "…"
        assertTrue(ApprovalReview.shortenedByLaptop(cut))
        assertFalse(ApprovalReview.shortenedByLaptop("cargo test --lib"))
        // A short command that merely ends in an ellipsis is not "cut by the laptop".
        assertFalse(ApprovalReview.shortenedByLaptop("echo hola…"))
    }

    @Test
    fun the_fit_limit_is_about_three_lines() {
        assertTrue(ApprovalReview.FITS_CHARS in 60..160)
    }
}
